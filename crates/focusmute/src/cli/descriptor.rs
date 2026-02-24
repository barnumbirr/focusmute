//! `descriptor` subcommand — dump raw descriptor bytes.

use super::{Result, ScarlettDevice, open_device};

pub(super) fn cmd_descriptor(offset: u32, size: u32) -> Result<()> {
    let device = open_device()?;
    cmd_descriptor_inner(&device, offset, size)
}

fn cmd_descriptor_inner(device: &impl ScarlettDevice, offset: u32, size: u32) -> Result<()> {
    let data = device.get_descriptor(offset, size)?;
    println!(
        "Descriptor [{offset}..{}] ({} bytes):",
        offset + size,
        data.len()
    );
    for (i, chunk) in data.chunks(16).enumerate() {
        let addr = offset as usize + i * 16;
        print!("  {addr:04X}: ");
        for b in chunk {
            print!("{b:02X} ");
        }
        println!();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use focusmute_lib::device::mock::MockDevice;

    #[test]
    fn descriptor_reads_data() {
        let dev = MockDevice::new();
        // Set some descriptor data at offset 0
        dev.set_descriptor(0, &[0xAB; 32]).unwrap();

        let result = cmd_descriptor_inner(&dev, 0, 32);
        assert!(result.is_ok());
    }

    #[test]
    fn descriptor_reads_partial_range() {
        let dev = MockDevice::new();
        dev.set_descriptor(100, &[0xCD; 16]).unwrap();

        let result = cmd_descriptor_inner(&dev, 100, 16);
        assert!(result.is_ok());
    }
}
