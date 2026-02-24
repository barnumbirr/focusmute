//! `map` subcommand — map directLEDValues by flashing one index at a time.

use super::{DeviceContext, Result, ScarlettDevice, layout, led, models, open_device};

/// Get hardcoded LED labels for a model, generated from profile + button names.
fn hardcoded_labels(model_name: &str) -> Option<Vec<String>> {
    let profile = models::detect_model(model_name)?;
    if profile.button_labels.is_empty() {
        return None;
    }
    Some(models::model_labels(profile, profile.button_labels))
}

pub(super) fn cmd_map(
    value: u8,
    delay: u64,
    index: Option<u8>,
    count: Option<usize>,
    output: Option<String>,
    output_code: bool,
    accept: bool,
) -> Result<()> {
    // Warn that LED state will be disrupted
    if !accept {
        use std::io::Write;
        println!("WARNING: This command puts the interface into direct LED mode.");
        println!("After it finishes, LEDs will be in a broken state (off or wrong colors).");
        println!("You will need to UNPLUG and REPLUG the device to restore normal LEDs.");
        println!();
        print!("Continue? [y/N] ");
        std::io::stdout().flush().ok();

        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).is_err()
            || !answer.trim().eq_ignore_ascii_case("y")
        {
            println!("Aborted.");
            return Ok(());
        }
        println!();
    }

    let device = open_device()?;
    let model = device.info().model().to_string();
    let ctx = DeviceContext::resolve(&device, true)?;

    // Auto-detect LED count from schema if not specified
    let led_count = count.unwrap_or(ctx.offsets.direct_led_count);

    // Resolve labels: hardcoded > predicted > generic
    let hardcoded = hardcoded_labels(&model);
    let labels = layout::resolve_labels(hardcoded.as_deref(), ctx.predicted.as_ref(), led_count);

    // Build a grayscale color from the brightness value (0-255)
    let v = value as u32;
    let color: u32 = (v << 24) | (v << 16) | (v << 8);

    println!("=== Map — directLEDValues[{led_count}] ===");
    println!(
        "Model: {model} | Brightness: {value} ({}) | Flash interval: {delay}s",
        led::format_color(color)
    );
    if ctx.predicted.is_some() && hardcoded.is_none() {
        println!("Labels predicted from firmware schema. Confirm or correct each one.");
    }
    println!("Each LED flashes. Confirm the label matches, or type a correction.");
    println!("Enter = correct, type new label = correction, q = quit.");
    println!();

    let led_bytes = led_count * 4;

    // Enable direct LED mode
    device
        .set_descriptor(ctx.offsets.enable_direct_led, &[2])
        .ok();

    let range: Box<dyn Iterator<Item = usize>> = match index {
        Some(i) => Box::new(std::iter::once(i as usize)),
        None => Box::new(0..led_count),
    };

    // Background stdin reader
    let (line_tx, line_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        loop {
            let mut buf = String::new();
            match stdin.read_line(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line_tx.send(buf).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let clear = vec![0u8; led_bytes];
    let mut corrections: Vec<(usize, String)> = Vec::new();

    for idx in range {
        if idx >= led_count {
            break;
        }

        let mut leds_on = vec![0u8; led_bytes];
        let off = idx * 4;
        leds_on[off..off + 4].copy_from_slice(&color.to_le_bytes());

        let (label, confidence) = &labels[idx];
        let conf_tag = match confidence {
            Some(c) => format!("[{c}]"),
            None => "[unknown]".to_string(),
        };

        use std::io::Write;
        print!("[{idx:>2}] {label:<36} {conf_tag:<12} ");
        std::io::stdout().flush().ok();

        let mut on = true;
        let quit = loop {
            if on {
                device
                    .set_descriptor(ctx.offsets.direct_led_values, &leds_on)
                    .ok();
            } else {
                device
                    .set_descriptor(ctx.offsets.direct_led_values, &clear)
                    .ok();
            }
            device.data_notify(ctx.offsets.direct_led_notify).ok();
            on = !on;

            if let Ok(line) = line_rx.try_recv() {
                let trimmed = line.trim();
                if trimmed.eq_ignore_ascii_case("q") {
                    break true;
                }
                if trimmed.is_empty() {
                    println!("OK");
                } else {
                    println!("CORRECTED -> {trimmed}");
                    corrections.push((idx, trimmed.to_string()));
                }
                break false;
            }

            std::thread::sleep(std::time::Duration::from_millis(delay * 1000 / 4));
        };

        if quit {
            break;
        }
    }

    println!();
    if corrections.is_empty() {
        println!("All labels confirmed correct!");
    } else {
        println!("Corrections needed:");
        for (idx, label) in &corrections {
            println!("  [{idx:>2}] {label}");
        }
    }

    // Apply corrections to predicted layout for output
    if let Some(ref pl) = ctx.predicted {
        if let Some(ref path) = output {
            let mut final_layout = pl.clone();
            for led in &mut final_layout.leds {
                if let Some((_, corrected)) = corrections.iter().find(|(i, _)| *i == led.index) {
                    led.label = corrected.clone();
                    led.confidence = layout::Confidence::High;
                }
            }
            match serde_json::to_string_pretty(&final_layout) {
                Ok(json) => match std::fs::write(path, &json) {
                    Ok(()) => println!("Layout saved to {path}"),
                    Err(e) => log::error!("writing {path}: {e}"),
                },
                Err(e) => log::error!("serializing layout: {e}"),
            }
        }

        if output_code {
            println!();
            println!("Model profile code:");
            println!("{}", layout::generate_model_profile_code(pl));
        }
    } else if output.is_some() || output_code {
        log::warn!("--output/--output-code require schema extraction (not available)");
    }

    println!();
    println!("Unplug and replug your interface to restore normal LED behavior.");
    println!("Done.");
    Ok(())
}
