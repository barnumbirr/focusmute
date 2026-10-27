//! LED device operations — single-LED mute indicator apply/clear/restore.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::device::{Result, ScarlettDevice};
use crate::offsets::DeviceOffsets;
use crate::schema::PreampGain;

use super::strategy::MuteStrategy;

// ── Single-LED update (DATA_NOTIFY(8)) ──

/// Set a single LED color via `directLEDColour` + `directLEDIndex` + DATA_NOTIFY.
///
/// Updates ONLY the targeted LED — zero side effects on any other LED.
/// Works in mode 0 (normal metering mode) without any mode change.
/// Metering continues unaffected on all halo ring segments.
///
/// `offsets` must be the connected device's own, from [`DeviceOffsets`]. The
/// three fields differ between models (the 2i2 writes colour at 84 and index at
/// 88; the Solo writes them at 80 and 84), and using another model's values
/// writes the colour into whatever field sits at that offset instead.
pub fn set_single_led(
    device: &impl ScarlettDevice,
    offsets: &DeviceOffsets,
    index: u8,
    color: u32,
) -> Result<()> {
    // Ordering matters: colour must be written before index.
    device.set_descriptor(offsets.direct_led_colour, &color.to_le_bytes())?;
    device.set_descriptor(offsets.direct_led_index, &[index])?;
    device.data_notify(offsets.direct_led_colour_notify)?;
    Ok(())
}

/// Restore number LEDs to their firmware-expected colors.
///
/// Reads `selectedInput` from the device to determine which input is currently
/// selected, then sets each number LED to the appropriate firmware color
/// (green for selected, white for unselected) via the single-LED write path.
///
/// Models without a `selectedInput` control have no active input to highlight,
/// so every number returns to the unselected colour. Reading the 2i2's offset
/// on such a device would return an unrelated field and pick a colour from it.
fn restore_number_leds(device: &impl ScarlettDevice, strategy: &MuteStrategy) -> Result<()> {
    let selected_input: Option<usize> = match strategy.offsets.selected_input {
        Some(offset) => Some(
            device
                .get_descriptor(offset, 1)?
                .first()
                .copied()
                .unwrap_or(0) as usize,
        ),
        None => None,
    };

    for (input_idx, &led_idx) in strategy
        .input_indices
        .iter()
        .zip(strategy.number_leds.iter())
    {
        let color = if selected_input == Some(*input_idx) {
            strategy.selected_color
        } else {
            strategy.unselected_color
        };
        set_single_led(device, &strategy.offsets, led_idx, color)?;
    }
    Ok(())
}

// ── Hardware preamp gain (the card's own input level) ──

/// Preamp gains saved while muted, as `(input index, dB)`. Empty when not muted.
/// Mirrored to `saved_gains.txt` so a crash while muted cannot lose them.
static SAVED_GAINS: Mutex<Vec<(usize, u8)>> = Mutex::new(Vec::new());

/// The mirror file. Unset (memory only) until a binary calls [`persist_saved_gains`],
/// so no test in any crate can read or write the user's real file.
static GAINS_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Mirror saved preamp gains to `saved_gains.txt` in the config directory.
/// Call once at startup from each binary.
pub fn persist_saved_gains() {
    if let Some(dir) = crate::config::Config::dir() {
        let _ = GAINS_FILE.set(dir.join("saved_gains.txt"));
    }
}

fn persist_gains(saved: &[(usize, u8)]) {
    let Some(path) = GAINS_FILE.get() else { return };
    let text: String = saved.iter().map(|(i, db)| format!("{i}:{db}\n")).collect();
    if let Err(e) = std::fs::write(path, text) {
        log::warn!("[gain] could not save {}: {e}", path.display());
    }
}

/// Gains left in the mirror file by a run that ended while muted.
fn load_gains() -> Vec<(usize, u8)> {
    let Some(text) = GAINS_FILE
        .get()
        .and_then(|p| std::fs::read_to_string(p).ok())
    else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let (i, db) = l.split_once(':')?;
            Some((i.parse().ok()?, db.parse().ok()?))
        })
        .collect()
}

/// Write one preamp gain through the parameter buffer + DATA_NOTIFY.
fn set_preamp_gain(
    device: &impl ScarlettDevice,
    pg: &PreampGain,
    input: usize,
    db: u8,
) -> Result<()> {
    device.set_descriptor(pg.channel_offset, &[input as u8])?;
    device.set_descriptor(pg.value_offset, &[db])?;
    device.data_notify(pg.notify)
}

/// Drop the muted inputs' preamp gain to 0 dB so the card sends no signal.
fn mute_preamp_gain(device: &impl ScarlettDevice, strategy: &MuteStrategy) -> Result<()> {
    let Some(pg) = strategy.offsets.preamp_gain else {
        return Ok(());
    };
    let mut saved = SAVED_GAINS.lock().unwrap_or_else(|e| e.into_inner());
    if saved.is_empty() {
        *saved = load_gains();
    }
    if !saved.is_empty() {
        return Ok(()); // already muted; keep the original gains
    }
    for &input in strategy.input_indices.iter().filter(|&&i| i < pg.count) {
        let gain = device.get_descriptor(pg.offset + input as u32, 1)?;
        saved.push((input, gain.first().copied().unwrap_or(0)));
        persist_gains(&saved);
        set_preamp_gain(device, &pg, input, 0)?;
    }
    Ok(())
}

/// Put the saved preamp gains back. A no-op when none are saved.
pub fn restore_preamp_gain(device: &impl ScarlettDevice, offsets: &DeviceOffsets) -> Result<()> {
    let Some(pg) = offsets.preamp_gain else {
        return Ok(());
    };
    let mut saved = SAVED_GAINS.lock().unwrap_or_else(|e| e.into_inner());
    if saved.is_empty() {
        *saved = load_gains();
    }
    while let Some(&(input, db)) = saved.last() {
        // A file left by another model may name inputs this one lacks.
        if input < pg.count {
            set_preamp_gain(device, &pg, input, db)?;
        }
        saved.pop();
        persist_gains(&saved);
    }
    Ok(())
}

// ── Mute indicator operations ──

/// Apply the mute indicator based on the resolved strategy.
///
/// Sets only the muted input number LEDs via DATA_NOTIFY(8).
/// No mode change, no gradient change — metering continues on all other LEDs.
pub fn apply_mute_indicator(
    device: &impl ScarlettDevice,
    strategy: &MuteStrategy,
    mute_color: u32,
) -> Result<()> {
    for (i, &led_idx) in strategy.number_leds.iter().enumerate() {
        let color = strategy.mute_colors.get(i).copied().unwrap_or(mute_color);
        set_single_led(device, &strategy.offsets, led_idx, color)?;
    }
    mute_preamp_gain(device, strategy)
}

/// Clear the mute indicator and restore normal LED state.
pub fn clear_mute_indicator(device: &impl ScarlettDevice, strategy: &MuteStrategy) -> Result<()> {
    restore_number_leds(device, strategy)?;
    restore_preamp_gain(device, &strategy.offsets)
}

/// Restore LED state on application exit.
pub fn restore_on_exit(device: &impl ScarlettDevice, strategy: &MuteStrategy) -> Result<()> {
    restore_number_leds(device, strategy)?;
    restore_preamp_gain(device, &strategy.offsets)
}

/// Re-apply mute indicator after reconnecting, if currently muted.
///
/// The caller is responsible for the `open_device()` call and logging —
/// this extracts only the post-connect mute re-application.
pub fn refresh_after_reconnect(
    device: &impl ScarlettDevice,
    strategy: &MuteStrategy,
    mute_color: u32,
    is_muted: bool,
) -> Result<()> {
    if is_muted {
        apply_mute_indicator(device, strategy, mute_color)?;
    } else {
        restore_preamp_gain(device, &strategy.offsets)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::mock::MockDevice;
    use crate::protocol::*;

    /// Helper to set up a mock device with selectedInput for restore tests.
    fn setup_device_with_selected_input(dev: &MockDevice, selected: u8) {
        dev.set_descriptor(OFF_SELECTED_INPUT, &[selected]).unwrap();
    }

    fn make_strategy_one_input() -> MuteStrategy {
        MuteStrategy {
            input_indices: vec![0],
            number_leds: vec![0],
            mute_colors: vec![],
            selected_color: 0x20FF_0000,
            unselected_color: 0x88FF_FF00,
            offsets: Default::default(),
        }
    }

    fn make_strategy_both_inputs() -> MuteStrategy {
        MuteStrategy {
            input_indices: vec![0, 1],
            number_leds: vec![0, 8],
            mute_colors: vec![],
            selected_color: 0x20FF_0000,
            unselected_color: 0x88FF_FF00,
            offsets: Default::default(),
        }
    }

    // ── set_single_led ──

    /// Offsets observed on a Scarlett Solo 4th Gen: the single-LED fields sit
    /// four bytes earlier than the 2i2's, and the bulk array starts where the
    /// 2i2 keeps its index field. See docs/13, "Other 4th Gen models".
    fn compact_layout_offsets() -> DeviceOffsets {
        DeviceOffsets {
            direct_led_colour: 80,
            direct_led_index: 84,
            direct_led_colour_notify: 8,
            direct_led_values: 88,
            direct_led_count: 32,
            selected_input: None,
            ..Default::default()
        }
    }

    /// Regression: the write path used the 2i2's constants for every model, so
    /// on a device with a compact APP_SPACE the colour landed in the index
    /// field and the index in the first bulk-array entry.
    #[test]
    fn set_single_led_uses_the_devices_own_offsets() {
        let dev = MockDevice::new();
        let offsets = compact_layout_offsets();
        let color = 0x1234_5600u32;

        set_single_led(&dev, &offsets, 4, color).unwrap();

        let descs = dev.descriptors.borrow();
        assert_eq!(
            u32::from_le_bytes(descs.get(&80).unwrap()[..4].try_into().unwrap()),
            color,
            "colour must be written at this device's directLEDColour offset"
        );
        assert_eq!(descs.get(&84).unwrap(), &[4]);
        // The 2i2's offsets must not be touched: 84 is this device's index
        // field (asserted above), and 88 is the start of its bulk array.
        assert!(
            !descs.contains_key(&OFF_DIRECT_LED_INDEX),
            "must not write the 2i2 index offset (88) — it is the bulk array here"
        );
        drop(descs);
        assert_eq!(dev.notifies.borrow().as_slice(), &[8]);
    }

    /// Regression: restore read `selectedInput` at the 2i2's offset 331 on
    /// every model. A device without that control has an unrelated field
    /// there, and whatever byte it held decided the restore colour.
    #[test]
    fn restore_without_selected_input_never_reads_the_2i2_offset() {
        let dev = MockDevice::new();
        // Seed the 2i2 offset with the value that would mark input 0 selected.
        // A device with no selectedInput must ignore it.
        setup_device_with_selected_input(&dev, 0);

        let strategy = MuteStrategy {
            input_indices: vec![0, 1],
            number_leds: vec![4, 12],
            mute_colors: vec![],
            selected_color: 0x20FF_0000,
            unselected_color: 0x88FF_FF00,
            offsets: compact_layout_offsets(),
        };

        clear_mute_indicator(&dev, &strategy).unwrap();

        // Both numbers restore to the unselected colour; neither is green.
        let descs = dev.descriptors.borrow();
        assert_eq!(
            u32::from_le_bytes(descs.get(&80).unwrap()[..4].try_into().unwrap()),
            strategy.unselected_color,
            "no selectedInput means no input is highlighted"
        );
        assert_eq!(descs.get(&84).unwrap(), &[12]);
        drop(descs);
        assert_eq!(dev.notifies.borrow().as_slice(), &[8, 8]);
    }

    /// The 2i2 keeps its existing behaviour: the selected input goes green.
    #[test]
    fn restore_with_selected_input_still_highlights_the_selected_number() {
        let dev = MockDevice::new();
        setup_device_with_selected_input(&dev, 1);
        let strategy = make_strategy_both_inputs();

        clear_mute_indicator(&dev, &strategy).unwrap();

        // Input 2 (index 1) is selected and written last.
        let descs = dev.descriptors.borrow();
        assert_eq!(descs.get(&OFF_DIRECT_LED_INDEX).unwrap(), &[8]);
        assert_eq!(
            u32::from_le_bytes(
                descs.get(&OFF_DIRECT_LED_COLOUR).unwrap()[..4]
                    .try_into()
                    .unwrap()
            ),
            strategy.selected_color
        );
    }

    #[test]
    fn set_single_led_writes_colour_index_notify() {
        let dev = MockDevice::new();
        let color = 0xFF00_0000u32;
        set_single_led(&dev, &DeviceOffsets::default(), 0, color).unwrap();

        let descs = dev.descriptors.borrow();

        // directLEDColour should be written
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        assert_eq!(u32::from_le_bytes(colour[..4].try_into().unwrap()), color);

        // directLEDIndex should be written
        let index = descs.get(&OFF_DIRECT_LED_INDEX).unwrap();
        assert_eq!(index, &[0]);

        // Should have sent NOTIFY_DIRECT_LED_COLOUR (8)
        let notifies = dev.notifies.borrow();
        assert!(notifies.contains(&NOTIFY_DIRECT_LED_COLOUR));
    }

    #[test]
    fn set_single_led_does_not_touch_mode_or_values() {
        let dev = MockDevice::new();
        set_single_led(&dev, &DeviceOffsets::default(), 0, 0xFF00_0000).unwrap();

        let descs = dev.descriptors.borrow();
        assert!(!descs.contains_key(&OFF_ENABLE_DIRECT_LED));
        assert!(!descs.contains_key(&OFF_DIRECT_LED_VALUES));
    }

    // ── apply_mute_indicator ──

    #[test]
    fn apply_mute_sets_only_number_led() {
        let dev = MockDevice::new();
        let strategy = make_strategy_one_input();
        let color = 0xFF00_0000u32;

        apply_mute_indicator(&dev, &strategy, color).unwrap();

        let descs = dev.descriptors.borrow();

        // directLEDColour should have mute color
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        assert_eq!(u32::from_le_bytes(colour[..4].try_into().unwrap()), color);

        // directLEDIndex should be 0 (input 1 number LED)
        let index = descs.get(&OFF_DIRECT_LED_INDEX).unwrap();
        assert_eq!(index, &[0]);

        // Mode should NOT be changed
        assert!(!descs.contains_key(&OFF_ENABLE_DIRECT_LED));

        // directLEDValues should NOT be changed
        assert!(!descs.contains_key(&OFF_DIRECT_LED_VALUES));

        // Only NOTIFY_DIRECT_LED_COLOUR (8) should have been sent
        let notifies = dev.notifies.borrow();
        assert!(notifies.contains(&NOTIFY_DIRECT_LED_COLOUR));
        assert!(!notifies.contains(&NOTIFY_DIRECT_LED_VALUES));
    }

    #[test]
    fn apply_mute_both_number_leds() {
        let dev = MockDevice::new();
        let strategy = make_strategy_both_inputs();
        let color = 0xFF00_0000u32;

        apply_mute_indicator(&dev, &strategy, color).unwrap();

        // Both LEDs should have been set
        let notifies = dev.notifies.borrow();
        assert_eq!(
            notifies
                .iter()
                .filter(|&&n| n == NOTIFY_DIRECT_LED_COLOUR)
                .count(),
            2,
            "should have sent 2 DATA_NOTIFY(8) events"
        );
    }

    #[test]
    fn apply_mute_uses_per_input_colors() {
        let dev = MockDevice::new();
        let strategy = MuteStrategy {
            input_indices: vec![0, 1],
            number_leds: vec![0, 8],
            mute_colors: vec![0x00FF_0000, 0x0000_FF00],
            selected_color: 0x20FF_0000,
            unselected_color: 0x88FF_FF00,
            offsets: Default::default(),
        };

        apply_mute_indicator(&dev, &strategy, 0xFF00_0000).unwrap();

        // Last written colour should be for input 2 (0x0000_FF00)
        let descs = dev.descriptors.borrow();
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        assert_eq!(
            u32::from_le_bytes(colour[..4].try_into().unwrap()),
            0x0000_FF00
        );
    }

    // ── clear_mute_indicator ──

    #[test]
    fn clear_mute_restores_number_led_selected() {
        let dev = MockDevice::new();
        let strategy = make_strategy_one_input();

        // Input 1 is selected
        setup_device_with_selected_input(&dev, 0);

        // Apply then clear
        apply_mute_indicator(&dev, &strategy, 0xFF00_0000).unwrap();
        clear_mute_indicator(&dev, &strategy).unwrap();

        let descs = dev.descriptors.borrow();

        // Number LED should be restored to selected color (green)
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        assert_eq!(
            u32::from_le_bytes(colour[..4].try_into().unwrap()),
            0x20FF_0000,
            "should restore to selected green"
        );

        // Mode should NOT have been touched
        assert!(!descs.contains_key(&OFF_ENABLE_DIRECT_LED));
    }

    #[test]
    fn clear_mute_restores_number_led_unselected() {
        let dev = MockDevice::new();
        let strategy = make_strategy_one_input();

        // Input 2 is selected (so input 1 is unselected)
        setup_device_with_selected_input(&dev, 1);

        apply_mute_indicator(&dev, &strategy, 0xFF00_0000).unwrap();
        clear_mute_indicator(&dev, &strategy).unwrap();

        let descs = dev.descriptors.borrow();

        // Number LED should be restored to unselected color (white)
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        assert_eq!(
            u32::from_le_bytes(colour[..4].try_into().unwrap()),
            0x88FF_FF00,
            "should restore to unselected (white)"
        );
    }

    #[test]
    fn clear_mute_both_inputs_correct_colors() {
        let dev = MockDevice::new();
        let strategy = make_strategy_both_inputs();

        // Input 1 is selected
        setup_device_with_selected_input(&dev, 0);

        apply_mute_indicator(&dev, &strategy, 0xFF00_0000).unwrap();
        clear_mute_indicator(&dev, &strategy).unwrap();

        // Should have sent 4 DATA_NOTIFY(8) events total (2 apply + 2 clear)
        let notifies = dev.notifies.borrow();
        assert_eq!(
            notifies
                .iter()
                .filter(|&&n| n == NOTIFY_DIRECT_LED_COLOUR)
                .count(),
            4,
        );

        // Last LED written was index 8 (input 2, unselected → white)
        let descs = dev.descriptors.borrow();
        let index = descs.get(&OFF_DIRECT_LED_INDEX).unwrap();
        assert_eq!(index, &[8]);
        let colour = descs.get(&OFF_DIRECT_LED_COLOUR).unwrap();
        assert_eq!(
            u32::from_le_bytes(colour[..4].try_into().unwrap()),
            0x88FF_FF00,
            "last LED restored should be unselected (white)"
        );
    }

    // ── restore_on_exit ──

    #[test]
    fn restore_on_exit_restores_via_single_led() {
        let dev = MockDevice::new();
        let strategy = make_strategy_one_input();

        // Input 1 is selected
        setup_device_with_selected_input(&dev, 0);

        restore_on_exit(&dev, &strategy).unwrap();

        let descs = dev.descriptors.borrow();

        // Should restore via DATA_NOTIFY(8), not bulk mode/values
        assert!(descs.contains_key(&OFF_DIRECT_LED_COLOUR));
        assert!(!descs.contains_key(&OFF_ENABLE_DIRECT_LED));
        assert!(!descs.contains_key(&OFF_DIRECT_LED_VALUES));
    }

    // ── refresh_after_reconnect ──

    #[test]
    fn refresh_after_reconnect_not_muted() {
        let dev = MockDevice::new();
        let strategy = make_strategy_both_inputs();

        refresh_after_reconnect(&dev, &strategy, 0xFF00_0000, false).unwrap();

        // Not muted → no directLEDColour written
        let descs = dev.descriptors.borrow();
        assert!(
            !descs.contains_key(&OFF_DIRECT_LED_COLOUR),
            "should not write directLEDColour when not muted"
        );
    }

    #[test]
    fn refresh_after_reconnect_muted() {
        let dev = MockDevice::new();
        let strategy = make_strategy_both_inputs();
        let mute_color = 0xFF00_0000u32;

        refresh_after_reconnect(&dev, &strategy, mute_color, true).unwrap();

        let descs = dev.descriptors.borrow();
        // Muted → directLEDColour should have been written with mute color
        let colour = descs
            .get(&OFF_DIRECT_LED_COLOUR)
            .expect("should write directLEDColour when muted");
        assert_eq!(
            u32::from_le_bytes(colour[..4].try_into().unwrap()),
            mute_color
        );

        // Should have sent NOTIFY_DIRECT_LED_COLOUR for both LEDs
        let notifies = dev.notifies.borrow();
        assert_eq!(
            notifies
                .iter()
                .filter(|&&n| n == NOTIFY_DIRECT_LED_COLOUR)
                .count(),
            2,
            "should send DATA_NOTIFY(8) for each number LED"
        );
    }

    #[test]
    fn refresh_after_reconnect_muted_apply_fails_returns_err() {
        let dev = MockDevice::new();
        let strategy = make_strategy_both_inputs();

        // Make set_descriptor fail
        dev.fail_set_descriptor.set(true);

        // Should propagate the error from apply_mute_indicator
        let result = refresh_after_reconnect(&dev, &strategy, 0xFF00_0000, true);
        assert!(result.is_err(), "should return Err when apply fails");
    }

    // ── preamp gain ──

    /// The only test that touches SAVED_GAINS, so parallel tests cannot race on it.
    #[test]
    fn mute_zeroes_preamp_gain_and_unmute_restores_it() {
        let dev = MockDevice::new();
        let pg = PreampGain {
            offset: 75,
            count: 2,
            notify: 12,
            value_offset: 252,
            channel_offset: 253,
        };
        let mut strategy = make_strategy_both_inputs();
        strategy.offsets.preamp_gain = Some(pg);
        dev.set_descriptor(pg.offset, &[12, 61]).unwrap();
        let gain_writes = || dev.notifies.borrow().iter().filter(|&&n| n == 12).count();

        apply_mute_indicator(&dev, &strategy, 0xFF00_0000).unwrap();
        assert_eq!(gain_writes(), 2);
        assert_eq!(dev.get_descriptor(pg.value_offset, 1).unwrap(), [0]);

        // A second mute must not save the zeroed gains over the real ones.
        apply_mute_indicator(&dev, &strategy, 0xFF00_0000).unwrap();
        assert_eq!(gain_writes(), 2);

        clear_mute_indicator(&dev, &strategy).unwrap();
        assert_eq!(gain_writes(), 4);
        // Restored last-in first-out, so input 0 (12 dB) is the final write.
        assert_eq!(dev.get_descriptor(pg.channel_offset, 1).unwrap(), [0]);
        assert_eq!(dev.get_descriptor(pg.value_offset, 1).unwrap(), [12]);

        // Nothing saved any more: restore is a no-op.
        restore_preamp_gain(&dev, &strategy.offsets).unwrap();
        assert_eq!(gain_writes(), 4);
    }

    // ── T3: set_single_led error propagation ──

    #[test]
    fn set_single_led_propagates_write_error() {
        let dev = MockDevice::new();
        dev.fail_set_descriptor.set(true);

        let result = set_single_led(&dev, &DeviceOffsets::default(), 0, 0xFF00_0000);
        assert!(result.is_err(), "should propagate set_descriptor error");
    }

    #[test]
    fn set_single_led_propagates_notify_error() {
        let dev = MockDevice::new();
        dev.fail_data_notify.set(true);

        let result = set_single_led(&dev, &DeviceOffsets::default(), 0, 0xFF00_0000);
        assert!(result.is_err(), "should propagate data_notify error");
    }
}
