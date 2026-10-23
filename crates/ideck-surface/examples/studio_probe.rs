//! Dump raw Stream Deck Studio HID reports.
//!
//! ```text
//! cargo run -p ideck-surface --example studio_probe
//! ```

use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = hidapi::HidApi::new()?;
    let mut found = 0;
    for device in api.device_list() {
        if device.vendor_id() != 0x0fd9 || device.product_id() != 0x00aa {
            continue;
        }
        found += 1;
        println!(
            "studio serial={:?} path={:?}",
            device.serial_number(),
            device.path()
        );
        let handle = device.open_device(&api)?;
        let mut buf = [0u8; 512];
        for _ in 0..5 {
            match handle.read_timeout(&mut buf, 2000)? {
                0 => {
                    println!("timeout");
                    break;
                }
                n => {
                    let hex: String = buf[..n.min(64)]
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect();
                    println!("report len={n} head={hex}");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if found == 0 {
        println!("no Stream Deck Studio HID device (0fd9:00aa)");
    }
    Ok(())
}
