//! Audio mute detection — trait + Windows WASAPI backend.

use std::fmt;

#[derive(Debug)]
pub enum AudioError {
    InitFailed(String),
    OperationFailed(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::InitFailed(e) => write!(f, "Audio init failed: {e}"),
            AudioError::OperationFailed(e) => write!(f, "Audio operation failed: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

pub type Result<T> = std::result::Result<T, AudioError>;

/// Monitors microphone mute state.
pub trait MuteMonitor {
    fn is_muted(&self) -> bool;
    fn set_muted(&self, muted: bool) -> Result<()>;
    /// Block until a mute state change event or timeout.
    /// Returns `true` if woken by an event, `false` on timeout.
    fn wait_for_change(&self, timeout: std::time::Duration) -> bool;
    /// Refresh cached mute state from the underlying audio system.
    /// Default is a no-op; PulseAudio overrides to re-query state.
    fn refresh(&self) {}
}

/// Wait on a `(Mutex<bool>, Condvar)` signal pair with a timeout.
///
/// Returns `true` if the signal was raised (the bool was set to `true`),
/// `false` on timeout. Resets the flag after reading.
///
/// Shared by all `MuteMonitor` implementations to avoid duplicating the
/// condvar+timeout pattern.
fn wait_on_signal(
    signal: &(std::sync::Mutex<bool>, std::sync::Condvar),
    timeout: std::time::Duration,
) -> bool {
    let (lock, cvar) = signal;
    if let Ok(mut guard) = lock.lock() {
        if !*guard {
            match cvar.wait_timeout(guard, timeout) {
                Ok((new_guard, _)) => guard = new_guard,
                Err(e) => guard = e.into_inner().0,
            }
        }
        let was_changed = *guard;
        *guard = false;
        was_changed
    } else {
        log::warn!("signal mutex poisoned — falling back to sleep");
        std::thread::sleep(timeout);
        false
    }
}

// ── Windows WASAPI implementation ──

#[cfg(windows)]
mod wasapi {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::*;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::STGM_READ;
    use windows::Win32::System::Com::*;
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::core::implement;

    /// COM callback that receives volume/mute change notifications.
    #[implement(IAudioEndpointVolumeCallback)]
    struct MuteCallback {
        muted: Arc<AtomicBool>,
        signal: Arc<(Mutex<bool>, Condvar)>,
    }

    impl IAudioEndpointVolumeCallback_Impl for MuteCallback_Impl {
        fn OnNotify(
            &self,
            pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
        ) -> windows::core::Result<()> {
            if !pnotify.is_null() {
                let muted = unsafe { (*pnotify).bMuted.as_bool() };
                self.muted.store(muted, Ordering::SeqCst);
                if let Ok(mut changed) = self.signal.0.lock() {
                    *changed = true;
                    self.signal.1.notify_all();
                }
            }
            Ok(())
        }
    }

    pub struct WasapiMonitor {
        volume: IAudioEndpointVolume,
        device_name: Option<String>,
        /// Cached mute state, updated by COM callback.
        muted: Arc<AtomicBool>,
        /// Signaled by COM callback when mute state changes.
        signal: Arc<(Mutex<bool>, Condvar)>,
        /// Must keep callback alive for the lifetime of the registration.
        _callback: IAudioEndpointVolumeCallback,
    }

    // COM pointers are Send-safe with proper initialization per thread.
    // The monitor is created and used on threads that call CoInitialize.
    unsafe impl Send for WasapiMonitor {}

    // SAFETY: The shared fields accessed from background threads — `muted`
    // (AtomicBool) and `signal` (Mutex+Condvar) — are inherently Sync.
    // The COM `volume` pointer is only used by `set_muted()`, which is
    // called exclusively from the main thread. `Drop` (which also touches
    // COM) runs on the main thread because `run_core` joins the background
    // thread before dropping the monitor.
    unsafe impl Sync for WasapiMonitor {}

    impl WasapiMonitor {
        /// Create a new monitor for the default capture (microphone) device.
        /// Caller must ensure COM is initialized on this thread.
        pub fn new() -> Result<Self> {
            unsafe {
                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                        .map_err(|e| AudioError::InitFailed(format!("MMDeviceEnumerator: {e}")))?;

                let device = enumerator
                    .GetDefaultAudioEndpoint(eCapture, eConsole)
                    .map_err(|e| AudioError::InitFailed(format!("GetDefaultAudioEndpoint: {e}")))?;

                // Query device friendly name from property store
                let device_name = match device.OpenPropertyStore(STGM_READ) {
                    Ok(store) => match store.GetValue(&PKEY_Device_FriendlyName) {
                        Ok(prop) => {
                            if prop.Anonymous.Anonymous.vt == VT_LPWSTR {
                                prop.Anonymous.Anonymous.Anonymous.pwszVal.to_string().ok()
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    },
                    Err(_) => None,
                };

                let volume: IAudioEndpointVolume = device
                    .Activate(CLSCTX_ALL, None)
                    .map_err(|e| AudioError::InitFailed(format!("IAudioEndpointVolume: {e}")))?;

                // Read initial mute state
                let initial_muted = volume.GetMute().map(|b| b.as_bool()).unwrap_or(false);

                let muted = Arc::new(AtomicBool::new(initial_muted));
                let signal = Arc::new((Mutex::new(false), Condvar::new()));

                // Register COM callback for mute/volume change events
                let callback: IAudioEndpointVolumeCallback = MuteCallback {
                    muted: Arc::clone(&muted),
                    signal: Arc::clone(&signal),
                }
                .into();
                volume.RegisterControlChangeNotify(&callback).map_err(|e| {
                    AudioError::InitFailed(format!("RegisterControlChangeNotify: {e}"))
                })?;

                Ok(WasapiMonitor {
                    volume,
                    device_name,
                    muted,
                    signal,
                    _callback: callback,
                })
            }
        }
    }

    impl WasapiMonitor {
        pub fn device_name(&self) -> Option<&str> {
            self.device_name.as_deref()
        }
    }

    impl MuteMonitor for WasapiMonitor {
        fn is_muted(&self) -> bool {
            self.muted.load(Ordering::SeqCst)
        }

        fn set_muted(&self, muted: bool) -> Result<()> {
            unsafe {
                self.volume
                    .SetMute(muted, std::ptr::null())
                    .map_err(|e| AudioError::OperationFailed(format!("SetMute: {e}")))
            }
        }

        fn wait_for_change(&self, timeout: Duration) -> bool {
            super::wait_on_signal(&self.signal, timeout)
        }
    }

    impl Drop for WasapiMonitor {
        fn drop(&mut self) {
            unsafe {
                let _ = self.volume.UnregisterControlChangeNotify(&self._callback);
            }
        }
    }

    /// Initialize COM for the current thread (apartment-threaded).
    pub fn com_init() -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| AudioError::InitFailed(format!("CoInitializeEx: {e}")))
        }
    }
}

#[cfg(windows)]
pub use wasapi::{WasapiMonitor, com_init};

// ── Linux PulseAudio implementation ──

#[cfg(target_os = "linux")]
mod pulse {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use libpulse_binding::callbacks::ListResult;
    use libpulse_binding::context::subscribe::InterestMaskSet;
    use libpulse_binding::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
    use libpulse_binding::mainloop::threaded::Mainloop;

    struct PulseInner {
        mainloop: Mainloop,
        context: Context,
    }

    pub struct PulseAudioMonitor {
        inner: Mutex<PulseInner>,
        muted: Arc<AtomicBool>,
        device_name: Arc<Mutex<Option<String>>>,
        /// Signaled when PulseAudio delivers a source change event.
        signal: Arc<(Mutex<bool>, Condvar)>,
    }

    // PulseAudio threaded mainloop is designed for concurrent access.
    // The Mutex<PulseInner> ensures safe mutable access from &self methods.
    unsafe impl Send for PulseAudioMonitor {}
    unsafe impl Sync for PulseAudioMonitor {}

    impl PulseAudioMonitor {
        /// Create a new monitor for the default PulseAudio/PipeWire source.
        ///
        /// Subscribes to source events and maintains a cached mute state.
        /// Source change events signal the condvar for event-driven wakeup.
        pub fn new() -> Result<Self> {
            let mut mainloop = Mainloop::new().ok_or_else(|| {
                AudioError::InitFailed("PulseAudio mainloop creation failed".into())
            })?;

            let mut context = Context::new(&mainloop, "focusmute").ok_or_else(|| {
                AudioError::InitFailed("PulseAudio context creation failed".into())
            })?;

            context
                .connect(None, ContextFlagSet::NOFLAGS, None)
                .map_err(|e| AudioError::InitFailed(format!("PulseAudio connect: {e}")))?;

            mainloop
                .start()
                .map_err(|e| AudioError::InitFailed(format!("PulseAudio mainloop start: {e}")))?;

            // Wait for context to be ready
            loop {
                mainloop.lock();
                let state = context.get_state();
                mainloop.unlock();
                match state {
                    ContextState::Ready => break,
                    ContextState::Failed | ContextState::Terminated => {
                        return Err(AudioError::InitFailed(
                            "PulseAudio context connection failed".into(),
                        ));
                    }
                    _ => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }

            let muted = Arc::new(AtomicBool::new(false));
            let device_name: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let signal = Arc::new((Mutex::new(false), Condvar::new()));

            // Subscribe to source events with callback that signals change
            {
                mainloop.lock();

                // Set subscribe callback — fires when any subscribed source event arrives.
                // Signals the condvar so the monitor loop wakes up immediately.
                let signal_cb = Arc::clone(&signal);
                context.set_subscribe_callback(Some(Box::new(
                    move |_facility, _operation, _index| {
                        if let Ok(mut changed) = signal_cb.0.lock() {
                            *changed = true;
                            signal_cb.1.notify_all();
                        }
                    },
                )));

                context.subscribe(InterestMaskSet::SOURCE, |_success| {});

                // Initial query for default source mute state and name
                let muted_init = Arc::clone(&muted);
                let name_init = Arc::clone(&device_name);
                let introspect = context.introspect();
                introspect.get_source_info_by_name("@DEFAULT_SOURCE@", move |result| {
                    if let ListResult::Item(info) = result {
                        muted_init.store(info.mute, Ordering::SeqCst);
                        if let Some(ref desc) = info.description
                            && let Ok(mut n) = name_init.lock()
                        {
                            *n = Some(desc.to_string());
                        }
                    }
                });

                mainloop.unlock();
            }

            Ok(PulseAudioMonitor {
                inner: Mutex::new(PulseInner { mainloop, context }),
                muted,
                device_name,
                signal,
            })
        }

        /// Refresh the cached mute state by querying PulseAudio.
        ///
        /// Call this after `wait_for_change` returns or periodically as a heartbeat.
        pub fn refresh(&self) {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let muted_clone = Arc::clone(&self.muted);
            let name_clone = Arc::clone(&self.device_name);
            inner.mainloop.lock();
            let introspect = inner.context.introspect();
            introspect.get_source_info_by_name("@DEFAULT_SOURCE@", move |result| {
                if let ListResult::Item(info) = result {
                    muted_clone.store(info.mute, Ordering::SeqCst);
                    if let Some(ref desc) = info.description
                        && let Ok(mut n) = name_clone.lock()
                    {
                        *n = Some(desc.to_string());
                    }
                }
            });
            inner.mainloop.unlock();
        }

        pub fn device_name(&self) -> Option<String> {
            self.device_name.lock().ok().and_then(|n| n.clone())
        }
    }

    impl MuteMonitor for PulseAudioMonitor {
        fn is_muted(&self) -> bool {
            self.muted.load(Ordering::SeqCst)
        }

        fn set_muted(&self, muted: bool) -> Result<()> {
            let mut inner = self.inner.lock().map_err(|e| {
                AudioError::OperationFailed(format!("PulseAudio mutex poisoned: {e}"))
            })?;
            inner.mainloop.lock();
            let mut introspect = inner.context.introspect();
            introspect.set_source_mute_by_name("@DEFAULT_SOURCE@", muted, None);
            inner.mainloop.unlock();
            self.muted.store(muted, Ordering::SeqCst);
            Ok(())
        }

        fn wait_for_change(&self, timeout: Duration) -> bool {
            super::wait_on_signal(&self.signal, timeout)
        }
    }

    impl Drop for PulseAudioMonitor {
        fn drop(&mut self) {
            if let Ok(mut inner) = self.inner.lock() {
                inner.mainloop.lock();
                inner.context.disconnect();
                inner.mainloop.unlock();
                inner.mainloop.stop();
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use pulse::PulseAudioMonitor;

/// Stabilize a newly-created PulseAudio monitor.
///
/// PulseAudio needs a brief delay after connection before the cached
/// mute state is reliable. This is a known PulseAudio quirk — the
/// initial state query may return stale data without this stabilization.
#[cfg(target_os = "linux")]
pub fn stabilize_pulseaudio(monitor: &PulseAudioMonitor) {
    std::thread::sleep(std::time::Duration::from_millis(50));
    monitor.refresh();
    std::thread::sleep(std::time::Duration::from_millis(50));
}

// ── Debounce filter ──

/// Debounce filter for mute state changes.
///
/// Requires `threshold` consecutive polls of a new state before reporting
/// a transition. This filters out transient flicker from COM/endpoint
/// lifecycle events.
pub struct MuteDebouncer {
    threshold: u32,
    current: bool,
    pending: bool,
    stable: u32,
}

impl MuteDebouncer {
    /// Create a new debouncer with the given threshold and initial state.
    pub fn new(threshold: u32, initial: bool) -> Self {
        MuteDebouncer {
            threshold,
            current: initial,
            pending: initial,
            stable: 0,
        }
    }

    /// Feed a new poll result. Returns `Some(new_state)` if the state has
    /// been stable for `threshold` consecutive polls, otherwise `None`.
    pub fn update(&mut self, muted: bool) -> Option<bool> {
        if muted != self.current {
            if muted == self.pending {
                self.stable += 1;
            } else {
                self.pending = muted;
                self.stable = 1;
            }
            if self.stable >= self.threshold {
                self.current = muted;
                self.stable = 0;
                return Some(muted);
            }
        } else {
            self.pending = self.current;
            self.stable = 0;
        }
        None
    }

    /// Current confirmed mute state.
    pub fn is_muted(&self) -> bool {
        self.current
    }

    /// Force the confirmed state without going through debounce.
    ///
    /// Use this when the mute state is known from an authoritative source
    /// (e.g. the audio API at startup) and you want to sync the debouncer
    /// to match without triggering a state-change event.
    pub fn force_state(&mut self, muted: bool) {
        self.current = muted;
        self.pending = muted;
        self.stable = 0;
    }
}

// ── Test stub ──

/// Scriptable [`MuteMonitor`] for unit and integration tests.
///
/// Holds a sequence of mute states; each call to [`is_muted`] pops the next
/// value. When the sequence is exhausted, the last value is repeated.
pub mod stub {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    pub struct StubMonitor {
        muted: AtomicBool,
        signal: (Mutex<bool>, Condvar),
    }

    impl StubMonitor {
        /// Create a new stub with the given initial mute state.
        pub fn new(initial_muted: bool) -> Self {
            Self {
                muted: AtomicBool::new(initial_muted),
                signal: (Mutex::new(false), Condvar::new()),
            }
        }

        /// Set the mute state and signal any thread waiting in `wait_for_change`.
        pub fn set(&self, muted: bool) {
            self.muted.store(muted, Ordering::SeqCst);
            if let Ok(mut changed) = self.signal.0.lock() {
                *changed = true;
                self.signal.1.notify_all();
            }
        }
    }

    impl MuteMonitor for StubMonitor {
        fn is_muted(&self) -> bool {
            self.muted.load(Ordering::SeqCst)
        }

        fn set_muted(&self, muted: bool) -> Result<()> {
            self.set(muted);
            Ok(())
        }

        fn wait_for_change(&self, timeout: Duration) -> bool {
            wait_on_signal(&self.signal, timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debouncer_needs_threshold_polls() {
        let mut d = MuteDebouncer::new(3, false);
        // First two polls of "true" should not trigger
        assert_eq!(d.update(true), None);
        assert_eq!(d.update(true), None);
        // Third consecutive poll triggers
        assert_eq!(d.update(true), Some(true));
        assert!(d.is_muted());
    }

    #[test]
    fn debouncer_resets_on_flicker() {
        let mut d = MuteDebouncer::new(3, false);
        assert_eq!(d.update(true), None);
        assert_eq!(d.update(true), None);
        // Flicker back to false resets
        assert_eq!(d.update(false), None);
        assert!(!d.is_muted());
        // Need 3 fresh polls of true again
        assert_eq!(d.update(true), None);
        assert_eq!(d.update(true), None);
        assert_eq!(d.update(true), Some(true));
    }

    #[test]
    fn debouncer_same_state_no_trigger() {
        let mut d = MuteDebouncer::new(3, false);
        // Polling the current state never triggers
        assert_eq!(d.update(false), None);
        assert_eq!(d.update(false), None);
        assert_eq!(d.update(false), None);
        assert!(!d.is_muted());
    }

    #[test]
    fn debouncer_threshold_one() {
        let mut d = MuteDebouncer::new(1, false);
        // Single poll should trigger immediately
        assert_eq!(d.update(true), Some(true));
    }

    #[test]
    fn debouncer_roundtrip() {
        let mut d = MuteDebouncer::new(2, false);
        // Go muted
        assert_eq!(d.update(true), None);
        assert_eq!(d.update(true), Some(true));
        // Go unmuted
        assert_eq!(d.update(false), None);
        assert_eq!(d.update(false), Some(false));
        assert!(!d.is_muted());
    }

    #[test]
    fn force_state_syncs_to_muted() {
        let mut d = MuteDebouncer::new(2, false);
        assert!(!d.is_muted());

        d.force_state(true);
        assert!(d.is_muted());

        // Subsequent true polls should not trigger (already confirmed muted)
        assert_eq!(d.update(true), None);
        assert_eq!(d.update(true), None);
    }

    #[test]
    fn force_state_syncs_to_unmuted() {
        let mut d = MuteDebouncer::new(2, true);
        assert!(d.is_muted());

        d.force_state(false);
        assert!(!d.is_muted());

        // Subsequent false polls should not trigger
        assert_eq!(d.update(false), None);
        assert_eq!(d.update(false), None);
    }

    #[test]
    fn force_state_resets_pending_debounce() {
        let mut d = MuteDebouncer::new(3, false);

        // Start debouncing towards muted (1 of 3 polls done)
        assert_eq!(d.update(true), None);

        // Force to muted — should reset pending state
        d.force_state(true);
        assert!(d.is_muted());

        // Now a single false poll should NOT trigger (needs 3 fresh)
        assert_eq!(d.update(false), None);
        assert_eq!(d.update(false), None);
        assert_eq!(d.update(false), Some(false));
    }

    // ── StubMonitor ──

    #[test]
    fn stub_monitor_initial_state() {
        let m = stub::StubMonitor::new(false);
        assert!(!m.is_muted());
        let m2 = stub::StubMonitor::new(true);
        assert!(m2.is_muted());
    }

    #[test]
    fn stub_monitor_set_changes_state() {
        let m = stub::StubMonitor::new(false);
        assert!(!m.is_muted());
        m.set(true);
        assert!(m.is_muted());
        m.set(false);
        assert!(!m.is_muted());
    }

    #[test]
    fn stub_monitor_set_muted_trait() {
        let m = stub::StubMonitor::new(false);
        m.set_muted(true).unwrap();
        assert!(m.is_muted());
        m.set_muted(false).unwrap();
        assert!(!m.is_muted());
    }

    #[test]
    fn stub_monitor_wait_for_change_signals() {
        use std::sync::Arc;
        use std::time::Duration;

        let m = Arc::new(stub::StubMonitor::new(false));
        let m2 = Arc::clone(&m);

        let handle = std::thread::spawn(move || m2.wait_for_change(Duration::from_secs(5)));

        // Small delay to ensure the wait thread is blocked
        std::thread::sleep(Duration::from_millis(20));
        m.set(true);

        let woken = handle.join().unwrap();
        assert!(woken, "should have been woken by set()");
        assert!(m.is_muted());
    }

    #[test]
    fn stub_monitor_wait_for_change_timeout() {
        let m = stub::StubMonitor::new(false);
        let woken = m.wait_for_change(std::time::Duration::from_millis(10));
        assert!(!woken, "should timeout without signal");
    }
}
