//! `devices` subcommand — list connected Focusrite devices.

use super::{DevicesOutput, Result, device};

pub(super) fn cmd_devices(json: bool) -> Result<()> {
    let devices = device::enumerate_devices();

    if json {
        let output = DevicesOutput {
            count: devices.len(),
            devices,
        };
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return Ok(());
    }

    if devices.is_empty() {
        println!("No Focusrite devices found.");
        return Ok(());
    }

    println!(
        "Found {} Focusrite device{}:",
        devices.len(),
        if devices.len() == 1 { "" } else { "s" }
    );
    println!();

    for (i, dev) in devices.iter().enumerate() {
        println!("  [{}] {}", i + 1, dev.path);
        if let Some(ref serial) = dev.serial {
            println!("      Serial: {serial}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_devices_text_succeeds() {
        // enumerate_devices returns empty on test hosts without hardware
        let result = cmd_devices(false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_devices_json_succeeds() {
        let result = cmd_devices(true);
        assert!(result.is_ok());
    }
}
