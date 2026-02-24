//! `probe` subcommand — probe device capabilities and extract firmware schema.

use super::{
    Result, ScarlettDevice, format_kv, kv, kv_indent, kv_width, layout, models, open_device, schema,
};

fn print_manual_template(model_name: &str, sc: Option<&schema::SchemaConstants>) {
    let led_count = sc.map_or_else(|| "???".to_string(), |s| s.direct_led_count.to_string());
    let input_count = sc.map_or_else(|| "???".to_string(), |s| s.max_inputs.to_string());

    println!("// Add to crates/focusmute-lib/src/models.rs:");
    println!(
        "// static {}: ModelProfile = ModelProfile {{",
        model_name.to_uppercase().replace(' ', "_")
    );
    println!("//     name: \"{model_name}\",");
    println!("//     input_count: {input_count},");
    println!("//     led_count: {led_count},");
    println!("//     input_halos: &[...],  // Run `focusmute-cli.exe map` to discover");
    println!("//     output_halo_segments: ...,");
    println!("// }};");
}

pub(super) fn cmd_probe(dump_schema: bool) -> Result<()> {
    let w = kv_width(
        &[
            "Device:",
            "Firmware:",
            "Serial:",
            "Hardcoded profile:",
            "Schema extraction:",
        ],
        &[
            "product_name:",
            "max_leds:",
            "max_inputs:",
            "max_outputs:",
            "gradient_count:",
            "gradient_offset:",
            "gradient_notify:",
            "direct_led_count:",
            "direct_led_offset:",
            "metering_segments:",
            "input_controls:",
            "app_space_features:",
            "Input LEDs:",
            "Output halo:",
            "Buttons:",
            "First button:",
        ],
    );

    let device = open_device()?;
    let info = device.info();

    kv("Device:", &info.device_name, w);
    kv("Firmware:", &info.firmware, w);
    if let Some(ref serial) = info.serial {
        kv("Serial:", serial, w);
    }
    println!();

    // Check hardcoded profile
    let profile = models::detect_model(info.model());
    if let Some(p) = profile {
        kv(
            "Hardcoded profile:",
            format_args!(
                "{} ({} inputs, {} LEDs)",
                p.name, p.input_count, p.led_count
            ),
            w,
        );
    } else {
        kv("Hardcoded profile:", "NOT FOUND", w);
    }
    println!();

    // Attempt schema extraction (may take several seconds for USB page reads)
    print!("{}", format_kv("Schema extraction:", "", w));
    use std::io::Write;
    std::io::stdout().flush().ok();

    // Progress dots while extraction runs
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done2 = done.clone();
    let progress_thread = std::thread::spawn(move || {
        while !done2.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !done2.load(std::sync::atomic::Ordering::Relaxed) {
                print!(".");
                std::io::stdout().flush().ok();
            }
        }
    });

    let schema_result = schema::extract_schema(&device);
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    progress_thread.join().ok();

    let schema_constants = match schema_result {
        Ok(sc) => {
            println!("OK");
            kv_indent("product_name:", &sc.product_name, w);
            kv_indent("max_leds:", sc.max_leds, w);
            kv_indent("max_inputs:", sc.max_inputs, w);
            kv_indent("max_outputs:", sc.max_outputs, w);
            kv_indent("gradient_count:", sc.gradient_count, w);
            kv_indent("gradient_offset:", sc.gradient_offset, w);
            kv_indent("gradient_notify:", sc.gradient_notify, w);
            kv_indent("direct_led_count:", sc.direct_led_count, w);
            kv_indent("direct_led_offset:", sc.direct_led_offset, w);
            kv_indent("metering_segments:", sc.metering_segments, w);
            if !sc.input_controls.is_empty() {
                kv_indent("input_controls:", sc.input_controls.join(", "), w);
            }
            if !sc.app_space_features.is_empty() {
                kv_indent("app_space_features:", sc.app_space_features.join(", "), w);
            }

            // Compare against hardcoded profile
            if let Some(p) = profile {
                println!();
                if p.led_count != sc.direct_led_count {
                    println!(
                        "  WARNING: led_count mismatch: profile={} schema={}",
                        p.led_count, sc.direct_led_count
                    );
                } else {
                    println!("  Profile matches schema.");
                }
            }

            Some(sc)
        }
        Err(e) => {
            println!("FAILED");
            println!("  Error: {e}");
            None
        }
    };
    println!();

    // Show predicted layout from schema
    if let Some(ref sc) = schema_constants {
        match layout::predict_layout(sc) {
            Ok(pl) => {
                println!("Predicted layout:");
                kv_indent(
                    "Input LEDs:",
                    format_args!(
                        "{} ({} inputs x {} LEDs/input)",
                        pl.input_count * layout::LEDS_PER_INPUT,
                        pl.input_count,
                        layout::LEDS_PER_INPUT,
                    ),
                    w,
                );
                kv_indent(
                    "Output halo:",
                    format_args!("{} segments", pl.output_halo_segments),
                    w,
                );
                kv_indent("Buttons:", format_args!("{} LEDs", pl.button_count), w);
                kv_indent("First button:", pl.first_button_index, w);
                println!();
            }
            Err(e) => {
                log::warn!("layout prediction failed: {e}");
                println!();
            }
        }
    }

    // Dump full schema JSON if requested
    if dump_schema {
        println!();
        match schema::read_schema_raw(&device) {
            Ok(raw) => match schema::decode_schema(&raw) {
                Ok(json) => {
                    println!("{json}");
                }
                Err(e) => log::error!("decoding schema: {e}"),
            },
            Err(e) => log::error!("reading schema: {e}"),
        }
    }

    // Print model profile template for unknown models
    if profile.is_none() {
        println!();
        println!("Model profile code:");

        // Use predicted layout for a complete code snippet when available
        if let Some(ref sc) = schema_constants {
            if let Ok(pl) = layout::predict_layout(sc) {
                println!("// Add to crates/focusmute-lib/src/models.rs:");
                println!("{}", layout::generate_model_profile_code(&pl));
            } else {
                print_manual_template(info.model(), schema_constants.as_ref());
            }
        } else {
            print_manual_template(info.model(), None);
        }
    }

    Ok(())
}
