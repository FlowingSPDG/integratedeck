//! Raspberry Pi GPIO bridge for a VSD2 virtual device.
//!
//! Build on Linux:
//! ```text
//! cargo build -p ideck-vsd --features gpio --bin ideck-vsd-gpio
//! ```
//!
//! Environment:
//! - `VSD_ADDR`      server address (default `127.0.0.1:28198`)
//! - `VSD_DEVICE`    virtual device id (default `integratedeck-primary`)
//! - `VSD_BUTTON_PINS` comma-separated BCM pins (default `17,27,22,23,24,25`)
//! - `VSD_IMAGE_DIR` directory for received key JPEGs (default `/tmp/ideck-vsd`)

#[cfg(all(target_os = "linux", feature = "gpio"))]
mod imp {
    use std::time::Duration;

    use ideck_vsd::client::VsdClient;
    use ideck_vsd::messages::{
        client_message, server_message, ClientInformation, ClientMessage, KeyEvent,
        OpenVirtualDevice,
    };
    use rppal::gpio::{Gpio, Level, PullUp};

    pub fn run() -> anyhow::Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async_main())
    }

    async fn async_main() -> anyhow::Result<()> {
        let addr = std::env::var("VSD_ADDR").unwrap_or_else(|_| "127.0.0.1:28198".into());
        let device = std::env::var("VSD_DEVICE").unwrap_or_else(|_| "integratedeck-primary".into());
        let pins: Vec<u8> = std::env::var("VSD_BUTTON_PINS")
            .unwrap_or_else(|_| "17,27,22,23,24,25".into())
            .split(',')
            .filter_map(|value| value.trim().parse().ok())
            .collect();
        let image_dir = std::env::var("VSD_IMAGE_DIR").unwrap_or_else(|_| "/tmp/ideck-vsd".into());
        std::fs::create_dir_all(&image_dir)?;

        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<KeyEvent>();
        let pins_for_thread = pins.clone();
        std::thread::spawn(move || {
            let gpio = match Gpio::new() {
                Ok(gpio) => gpio,
                Err(error) => {
                    eprintln!("gpio init failed: {error}");
                    return;
                }
            };
            let mut inputs = Vec::new();
            for pin in &pins_for_thread {
                match gpio.get(*pin).and_then(|pin| pin.into_input_pullup()) {
                    Ok(input) => inputs.push(input),
                    Err(error) => eprintln!("gpio {pin} failed: {error}"),
                }
            }
            let mut previous: Vec<bool> = inputs.iter().map(|_| false).collect();
            loop {
                for (index, input) in inputs.iter().enumerate() {
                    let pressed = input.read() == Level::Low;
                    if pressed != previous[index] {
                        previous[index] = pressed;
                        let _ = event_tx.send(KeyEvent {
                            index: index as u32,
                            pressed,
                        });
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        let (mut client, hello) = VsdClient::connect(
            &format!("ws://{addr}"),
            ClientInformation {
                name: "integratedeck-gpio".into(),
                user_agent: "ideck-gpio/0.1".into(),
                device_type: 2,
                device_model: "raspberry-pi".into(),
                os_name: "linux".into(),
                os_version: String::new(),
            },
        )
        .await?;
        println!(
            "hello host={} needs_auth={}",
            hello.hostname, hello.client_needs_authentication
        );
        client
            .send(&ClientMessage {
                msg: Some(client_message::Msg::OpenVirtualDevice(OpenVirtualDevice {
                    device_id: device,
                })),
            })
            .await?;

        loop {
            tokio::select! {
                frame = client.recv() => {
                    match frame? {
                        Some(message) => match message.msg {
                            Some(server_message::Msg::KeypadConfig(config)) => {
                                println!("grid {}x{}", config.rows, config.columns);
                            }
                            Some(server_message::Msg::ButtonImage(image)) => {
                                let path = format!("{image_dir}/key-{}.jpg", image.index);
                                std::fs::write(&path, &image.jpeg)?;
                                println!("image {} -> {path}", image.index);
                            }
                            _ => {}
                        },
                        None => break,
                    }
                }
                event = event_rx.recv() => {
                    let Some(event) = event else { break };
                    client
                        .send(&ClientMessage {
                            msg: Some(client_message::Msg::KeyEvent(event)),
                        })
                        .await?;
                }
            }
        }
        Ok(())
    }
}

fn main() {
    #[cfg(all(target_os = "linux", feature = "gpio"))]
    {
        if let Err(error) = imp::run() {
            eprintln!("ideck-vsd-gpio: {error:#}");
            std::process::exit(1);
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "gpio")))]
    {
        eprintln!("ideck-vsd-gpio requires Linux and --features gpio");
    }
}
