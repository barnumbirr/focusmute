//! Integration tests: end-to-end mute sequences using MockDevice.
//!
//! These tests exercise the full mute → unmute → restore cycle
//! through the public API, verifying that descriptor writes and notify
//! events happen in the correct order using the single-LED mechanism.

use focusmute_lib::device::ScarlettDevice;
use focusmute_lib::device::mock::MockDevice;
use focusmute_lib::led;
use focusmute_lib::monitor::{MonitorAction, MuteIndicator};
use focusmute_lib::protocol::*;

/// Helper: make a PerInput strategy for both inputs of a 2i2.
fn make_strategy_both() -> led::MuteStrategy {
    led::MuteStrategy {
        input_indices: vec![0, 1],
        number_leds: vec![0, 8],
        mute_colors: vec![],
        selected_color: 0x20FF_0000,
        unselected_color: 0x88FF_FF00,
    }
}

/// Helper: make a PerInput strategy for input 1 only.
fn make_strategy_input1() -> led::MuteStrategy {
    led::MuteStrategy {
        input_indices: vec![0],
        number_leds: vec![0],
        mute_colors: vec![],
        selected_color: 0x20FF_0000,
        unselected_color: 0x88FF_FF00,
    }
}

/// Helper: set up a MockDevice with selectedInput for restore.
fn setup_device(dev: &MockDevice, selected_input: u8) {
    dev.set_descriptor(OFF_SELECTED_INPUT, &[selected_input])
        .unwrap();
}

// ── Test: full mute → unmute cycle via single-LED ──

#[test]
fn full_mute_unmute_cycle() {
    let dev = MockDevice::new();
    setup_device(&dev, 0); // Input 1 selected
    let strategy = make_strategy_both();
    let mute_color = 0xFF00_0000u32;

    // 1. Apply mute — both number LEDs should change
    led::apply_mute_indicator(&dev, &strategy, mute_color).unwrap();

    let descs = dev.descriptors.borrow();
    let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
    let last_color = u32::from_le_bytes(colour[..4].try_into().unwrap());
    assert_eq!(last_color, mute_color);
    drop(descs);

    let notifies = dev.notifies.borrow();
    assert_eq!(
        notifies
            .iter()
            .filter(|&&n| n == NOTIFY_DIRECT_LED_COLOUR)
            .count(),
        2,
        "should send DATA_NOTIFY(8) for each input"
    );
    drop(notifies);

    // 2. Clear mute — restore number LEDs
    dev.notifies.borrow_mut().clear();
    led::clear_mute_indicator(&dev, &strategy).unwrap();

    let descs = dev.descriptors.borrow();
    // Last written should be input 2 (unselected → white)
    let index = descs.get(&OFF_DIRECT_LED_INDEX).unwrap();
    assert_eq!(index, &[8]);
    let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
    let restored_color = u32::from_le_bytes(colour[..4].try_into().unwrap());
    assert_eq!(
        restored_color, 0x88FF_FF00,
        "input 2 should restore to unselected (white)"
    );
    drop(descs);

    // Should NOT have used bulk direct LED mode
    let notifies = dev.notifies.borrow();
    assert!(
        !notifies.contains(&NOTIFY_DIRECT_LED_VALUES),
        "should not use bulk direct mode"
    );
    assert_eq!(
        notifies
            .iter()
            .filter(|&&n| n == NOTIFY_DIRECT_LED_COLOUR)
            .count(),
        2,
        "should send DATA_NOTIFY(8) for each input restore"
    );
}

// ── Test: rapid mute/unmute cycles ──

#[test]
fn rapid_mute_unmute_10_cycles() {
    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();
    let mute_color = 0xFF00_0000u32;

    for cycle in 0..10 {
        // Mute
        led::apply_mute_indicator(&dev, &strategy, mute_color).unwrap();
        let descs = dev.descriptors.borrow();
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        let v = u32::from_le_bytes(colour[..4].try_into().unwrap());
        assert_eq!(
            v, mute_color,
            "cycle {cycle} mute: color should be mute_color"
        );
        drop(descs);

        // Unmute
        led::clear_mute_indicator(&dev, &strategy).unwrap();
        let descs = dev.descriptors.borrow();
        // Input 1 is selected, so last written (input 2) should be white (unselected)
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        let v = u32::from_le_bytes(colour[..4].try_into().unwrap());
        assert_eq!(
            v, 0x88FF_FF00,
            "cycle {cycle} unmute: input 2 should restore to white"
        );
        drop(descs);
    }
}

// ── Test: MuteIndicator state machine integration ──

#[test]
fn mute_indicator_full_sequence() {
    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    let mut indicator = MuteIndicator::new(2, false, 0xFF00_0000, strategy);
    assert!(!indicator.is_muted());

    // Feed 2 muted polls to trigger ApplyMute (threshold=2)
    assert_eq!(indicator.update(true), MonitorAction::NoChange);
    assert_eq!(indicator.update(true), MonitorAction::ApplyMute);
    assert!(indicator.is_muted());

    // Apply on device
    indicator.apply_mute(&dev).unwrap();
    let descs = dev.descriptors.borrow();
    assert!(descs.contains_key(&OFF_DIRECT_LED_COLOUR));
    drop(descs);

    // Feed 2 unmuted polls to trigger ClearMute (threshold=2)
    assert_eq!(indicator.update(false), MonitorAction::NoChange);
    assert_eq!(indicator.update(false), MonitorAction::ClearMute);
    assert!(!indicator.is_muted());

    // Clear on device
    indicator.clear_mute(&dev).unwrap();
    // Verify restore used single-LED, not bulk mode
    let notifies = dev.notifies.borrow();
    assert!(
        !notifies.contains(&NOTIFY_DIRECT_LED_VALUES),
        "should not use bulk direct mode"
    );
}

// ── Test: full pipeline — StubMonitor → MuteIndicator → MockDevice ──

#[test]
fn full_pipeline_stub_monitor_to_device() {
    use focusmute_lib::audio::MuteMonitor;
    use focusmute_lib::audio::stub::StubMonitor;

    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    let monitor = StubMonitor::new(false);
    let mut indicator = MuteIndicator::new(2, false, 0xFF00_0000, strategy);

    // Simulate: monitor detects mute
    monitor.set(true);
    for _ in 0..2 {
        let muted = monitor.is_muted();
        match indicator.update(muted) {
            MonitorAction::ApplyMute => {
                indicator.apply_mute(&dev).unwrap();
            }
            MonitorAction::ClearMute => {
                indicator.clear_mute(&dev).unwrap();
            }
            MonitorAction::NoChange => {}
        }
    }
    assert!(indicator.is_muted());

    // Verify single-LED writes happened
    let descs = dev.descriptors.borrow();
    assert!(descs.contains_key(&OFF_DIRECT_LED_COLOUR));
    drop(descs);

    // Simulate: monitor detects unmute
    monitor.set(false);
    for _ in 0..2 {
        let muted = monitor.is_muted();
        match indicator.update(muted) {
            MonitorAction::ApplyMute => {
                indicator.apply_mute(&dev).unwrap();
            }
            MonitorAction::ClearMute => {
                indicator.clear_mute(&dev).unwrap();
            }
            MonitorAction::NoChange => {}
        }
    }
    assert!(!indicator.is_muted());

    // Verify restore used single-LED
    let notifies = dev.notifies.borrow();
    assert!(
        !notifies.contains(&NOTIFY_DIRECT_LED_VALUES),
        "should not use bulk direct mode"
    );
}

/// Simulates a realistic poll loop where the monitor state is sampled
/// repeatedly — tests that debouncing interleaves correctly with LED writes.
#[test]
fn pipeline_debounce_with_flicker() {
    use focusmute_lib::audio::MuteMonitor;
    use focusmute_lib::audio::stub::StubMonitor;

    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    let monitor = StubMonitor::new(false);
    let mut indicator = MuteIndicator::new(2, false, 0xFF00_0000, strategy);

    // Helper: run one poll cycle
    let poll = |monitor: &StubMonitor, indicator: &mut MuteIndicator, dev: &MockDevice| {
        let muted = monitor.is_muted();
        match indicator.update(muted) {
            MonitorAction::ApplyMute => {
                indicator.apply_mute(dev).unwrap();
            }
            MonitorAction::ClearMute => {
                indicator.clear_mute(dev).unwrap();
            }
            MonitorAction::NoChange => {}
        }
    };

    // 1 muted poll, then flicker back to unmuted
    monitor.set(true);
    poll(&monitor, &mut indicator, &dev);
    monitor.set(false); // flicker
    poll(&monitor, &mut indicator, &dev);

    // Should NOT have triggered mute (debounce reset)
    assert!(!indicator.is_muted());

    // No directLEDColour writes should have happened
    let descs = dev.descriptors.borrow();
    assert!(
        !descs.contains_key(&OFF_DIRECT_LED_COLOUR),
        "no LED writes should happen during flicker"
    );
    drop(descs);

    // Now 2 stable muted polls (threshold=2)
    monitor.set(true);
    poll(&monitor, &mut indicator, &dev);
    poll(&monitor, &mut indicator, &dev);
    assert!(indicator.is_muted());

    // Now directLEDColour should have been written
    let descs = dev.descriptors.borrow();
    assert!(
        descs.contains_key(&OFF_DIRECT_LED_COLOUR),
        "LED writes should happen after stable mute"
    );
}

/// Tests the pipeline with per-input mute strategy targeting input 1 only.
#[test]
fn pipeline_per_input_strategy() {
    use focusmute_lib::audio::MuteMonitor;
    use focusmute_lib::audio::stub::StubMonitor;

    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_input1();

    let monitor = StubMonitor::new(false);
    let mut indicator = MuteIndicator::new(2, false, 0xFF00_0000, strategy);

    // Mute via monitor
    monitor.set(true);
    for _ in 0..2 {
        let muted = monitor.is_muted();
        if let MonitorAction::ApplyMute = indicator.update(muted) {
            indicator.apply_mute(&dev).unwrap();
        }
    }
    assert!(indicator.is_muted());

    // Verify single-LED update was used
    let descs = dev.descriptors.borrow();
    assert!(
        descs.contains_key(&OFF_DIRECT_LED_COLOUR),
        "should have written directLEDColour"
    );
    // Only 1 DATA_NOTIFY(8) (single input)
    drop(descs);
    let notifies = dev.notifies.borrow();
    assert_eq!(
        notifies
            .iter()
            .filter(|&&n| n == NOTIFY_DIRECT_LED_COLOUR)
            .count(),
        1,
        "should send exactly 1 DATA_NOTIFY(8) for single input"
    );
    drop(notifies);

    // Unmute via monitor
    monitor.set(false);
    for _ in 0..2 {
        let muted = monitor.is_muted();
        if let MonitorAction::ClearMute = indicator.update(muted) {
            indicator.clear_mute(&dev).unwrap();
        }
    }
    assert!(!indicator.is_muted());

    // Verify restore used single-LED (DATA_NOTIFY(8)), not bulk mode
    let notifies = dev.notifies.borrow();
    assert!(
        !notifies.contains(&NOTIFY_DIRECT_LED_VALUES),
        "should not have used bulk direct LED mode"
    );
}

/// Tests set_muted() through the StubMonitor (simulating toggle hotkey).
#[test]
fn pipeline_toggle_via_set_muted() {
    use focusmute_lib::audio::MuteMonitor;
    use focusmute_lib::audio::stub::StubMonitor;

    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    let monitor = StubMonitor::new(false);
    let mut indicator = MuteIndicator::new(1, false, 0xFF00_0000, strategy);

    // Toggle mute (like hotkey would) — threshold=1 for instant transition
    let current = monitor.is_muted();
    monitor.set_muted(!current).unwrap();

    let muted = monitor.is_muted();
    assert!(muted);
    if let MonitorAction::ApplyMute = indicator.update(muted) {
        indicator.apply_mute(&dev).unwrap();
    }

    // Verify mute applied via single-LED
    let descs = dev.descriptors.borrow();
    assert!(descs.contains_key(&OFF_DIRECT_LED_COLOUR));
    drop(descs);

    // Toggle unmute
    let current = monitor.is_muted();
    monitor.set_muted(!current).unwrap();

    let muted = monitor.is_muted();
    assert!(!muted);
    if let MonitorAction::ClearMute = indicator.update(muted) {
        indicator.clear_mute(&dev).unwrap();
    }

    // Verify restore via single-LED
    let notifies = dev.notifies.borrow();
    assert!(
        !notifies.contains(&NOTIFY_DIRECT_LED_VALUES),
        "should not use bulk direct mode"
    );
}

// ── Test: MuteIndicator initial mute on start ──

#[test]
fn mute_indicator_initial_muted_state() {
    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    // Start already muted
    let indicator = MuteIndicator::new(2, true, 0xFF00_0000, strategy);
    assert!(indicator.is_muted());

    // Apply initial mute
    indicator.apply_mute(&dev).unwrap();

    let descs = dev.descriptors.borrow();
    let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
    let v = u32::from_le_bytes(colour[..4].try_into().unwrap());
    assert_eq!(v, 0xFF00_0000);
}

// ── Test: poll_and_apply integration ──

#[test]
fn poll_and_apply_full_cycle() {
    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    let mut indicator = MuteIndicator::new(2, false, 0xFF00_0000, strategy);

    // Mute (threshold=2)
    let (a1, e1) = indicator.poll_and_apply(true, &dev);
    assert_eq!(a1, MonitorAction::NoChange);
    assert!(e1.is_none());

    let (a2, e2) = indicator.poll_and_apply(true, &dev);
    assert_eq!(a2, MonitorAction::ApplyMute);
    assert!(e2.is_none());
    assert!(indicator.is_muted());

    // Unmute (threshold=2)
    let (a3, e3) = indicator.poll_and_apply(false, &dev);
    assert_eq!(a3, MonitorAction::NoChange);
    assert!(e3.is_none());

    let (a4, e4) = indicator.poll_and_apply(false, &dev);
    assert_eq!(a4, MonitorAction::ClearMute);
    assert!(e4.is_none());
    assert!(!indicator.is_muted());
}

// ── Test: restore_on_exit ──

#[test]
fn restore_on_exit_uses_single_led() {
    let dev = MockDevice::new();
    setup_device(&dev, 0);
    let strategy = make_strategy_both();

    led::restore_on_exit(&dev, &strategy).unwrap();

    let descs = dev.descriptors.borrow();
    // Should restore via DATA_NOTIFY(8), not bulk mode/values
    assert!(descs.contains_key(&OFF_DIRECT_LED_COLOUR));
    assert!(!descs.contains_key(&OFF_ENABLE_DIRECT_LED));
    assert!(!descs.contains_key(&OFF_DIRECT_LED_VALUES));
}
