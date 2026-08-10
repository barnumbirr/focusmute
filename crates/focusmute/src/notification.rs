//! Desktop notification abstraction with toast replacement support.
//!
//! Windows: WinRT `ToastNotification` with `SetTag()` for replacement.
//! Linux: `notify-rust` with `NotificationHandle::update()` for replacement.
//!
//! Mute-state notifications ("Microphone Muted" / "Microphone Live") replace
//! the previous toast so rapid mute/unmute doesn't stack up notifications.
//! Other notifications (startup warnings, duplicate instance) are fire-and-forget.

// ── Windows implementation ──

#[cfg(windows)]
mod platform {
    /// Tag used for mute-state notifications so new toasts replace previous ones.
    const MUTE_TAG: &str = "focusmute-mute";
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;

    pub struct Notifier {
        app_id: HSTRING,
    }

    impl Notifier {
        pub fn new() -> Self {
            Self {
                app_id: HSTRING::from(crate::tray::AUMID),
            }
        }

        /// Show a mute-state notification that replaces the previous one.
        pub fn show_mute_state(&self, body: &str) {
            if let Err(e) = self.show_tagged(body, MUTE_TAG) {
                log::warn!("[notification] toast failed: {e}");
            }
        }

        /// Fire-and-forget notification (startup warnings, duplicate instance).
        pub fn show_oneshot(body: &str) {
            if let Err(e) = Self::show_untagged(body) {
                log::warn!("[notification] toast failed: {e}");
            }
        }

        fn show_tagged(&self, body: &str, tag: &str) -> windows::core::Result<()> {
            let xml = Self::build_xml(body)?;
            let toast = ToastNotification::CreateToastNotification(&xml)?;
            toast.SetTag(&HSTRING::from(tag))?;
            let notifier = ToastNotificationManager::CreateToastNotifierWithId(&self.app_id)?;
            notifier.Show(&toast)?;
            Ok(())
        }

        fn show_untagged(body: &str) -> windows::core::Result<()> {
            let xml = Self::build_xml(body)?;
            let toast = ToastNotification::CreateToastNotification(&xml)?;
            let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
                crate::tray::AUMID,
            ))?;
            notifier.Show(&toast)?;
            Ok(())
        }

        fn build_xml(body: &str) -> windows::core::Result<XmlDocument> {
            let escaped = escape_xml(body);
            let xml_str = format!(
                "<toast>\
                   <visual>\
                     <binding template=\"ToastGeneric\">\
                       <text>FocusMute</text>\
                       <text>{escaped}</text>\
                     </binding>\
                   </visual>\
                   <audio silent=\"true\" />\
                 </toast>"
            );
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml_str))?;
            Ok(doc)
        }
    }

    /// Escape XML special characters in toast body text.
    fn escape_xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

// ── Linux implementation ──

#[cfg(target_os = "linux")]
mod platform {
    use std::cell::RefCell;

    pub struct Notifier {
        /// Stored handle from the last mute-state notification for in-place replacement.
        handle: RefCell<Option<notify_rust::NotificationHandle>>,
    }

    impl Notifier {
        pub fn new() -> Self {
            Self {
                handle: RefCell::new(None),
            }
        }

        /// Show a mute-state notification that replaces the previous one.
        pub fn show_mute_state(&self, body: &str) {
            let mut handle = self.handle.borrow_mut();
            if let Some(h) = handle.as_mut() {
                // Update existing notification in-place (D-Bus replaces_id).
                h.body(body);
                match h.update() {
                    Ok(_) => return,
                    // Stale handle (e.g. the notification daemon restarted) —
                    // fall through and show a fresh notification instead.
                    Err(e) => log::warn!("[notification] update failed, reshowing: {e}"),
                }
            }
            // First notification (or stale handle) — store for future replacements.
            match Self::build_notification(body).show() {
                Ok(h) => *handle = Some(h),
                Err(e) => log::warn!("[notification] failed: {e}"),
            }
        }

        /// Fire-and-forget notification (startup warnings, duplicate instance).
        pub fn show_oneshot(body: &str) {
            let _ = Self::build_notification(body).show();
        }

        fn build_notification(body: &str) -> notify_rust::Notification {
            let mut n = notify_rust::Notification::new();
            n.summary("FocusMute");
            if let Some(ref icon) = crate::icon::notification_icon_path() {
                n.icon(icon);
            }
            n.body(body);
            n
        }
    }
}

pub use platform::Notifier;
