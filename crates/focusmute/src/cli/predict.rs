//! `predict` subcommand — predict LED layout from a schema JSON file.

use super::{Result, kv, kv_width, layout, schema};

pub(super) fn cmd_predict(schema_file: String, json_output: bool) -> Result<()> {
    let contents = std::fs::read_to_string(&schema_file)?;
    let sc = schema::parse_schema(&contents)?;
    let pl = layout::predict_layout(&sc)?;

    if json_output {
        match serde_json::to_string_pretty(&pl) {
            Ok(json) => println!("{json}"),
            Err(e) => log::error!("serializing layout: {e}"),
        }
    } else {
        let w = kv_width(
            &[
                "Product:",
                "Total LEDs:",
                "Inputs:",
                "Output halo:",
                "Buttons:",
            ],
            &[],
        );
        kv("Product:", &pl.product_name, w);
        kv("Total LEDs:", pl.total_leds, w);
        kv("Inputs:", pl.input_count, w);
        kv(
            "Output halo:",
            format_args!("{} segments", pl.output_halo_segments),
            w,
        );
        kv("Buttons:", pl.button_count, w);
        println!();
        println!("LED map:");
        for led in &pl.leds {
            println!(
                "  [{:>2}] {:<36} [{}]",
                led.index, led.label, led.confidence
            );
        }
        println!();
        println!("Model profile code:");
        println!("{}", layout::generate_model_profile_code(&pl));
    }
    Ok(())
}
