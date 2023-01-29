use futures::executor::block_on;
use gpiod::{Chip, EdgeDetect, Options};
use paho_mqtt as mqtt; // using paho-mqtt as a client as it's sponsored by the Eclipse foundation
use serde::Deserialize;
use serde_yaml;
use std::{convert::TryInto, process, thread};

#[derive(Deserialize)]
struct GpioPin {
    name: String,
    header_pin: u32,
}

#[derive(Deserialize)]
struct GpioChip {
    path: String,
    pins: Vec<GpioPin>,
}

#[derive(Deserialize)]
struct Mqtt {
    topic: String,
    host: String,
}

#[derive(Deserialize)]
struct Config {
    mqtt: Mqtt,
    gpiochip: Vec<GpioChip>,
}

fn connect_to_mqtt(mqtt: &Mqtt) -> std::io::Result<mqtt::AsyncClient> {
    let client = mqtt::AsyncClient::new(mqtt.host.to_string()).unwrap_or_else(|err| {
        panic!("Could create MQTT broker for: {}; error {}", mqtt.host, err);
    });

    if let Err(err) = block_on(async {
        println!("Connecting to MQTT broker: {}", mqtt.host);
        client.connect(None).await?;
        Ok::<(), mqtt::Error>(())
    }) {
        eprintln!(
            "Could not connect to MQTT broker: {}; error {}",
            mqtt.host, err
        );
        process::exit(1);
    }
    Ok(client)
}

fn main() -> std::io::Result<()> {
    env_logger::init();
    println!("Starting move-detect...");
    let cfg_file = std::fs::File::open("/etc/move-detect.yaml").unwrap_or_else(|_| {
        panic!("Could not open config file: /etc/move-detect.yaml");
    });
    let config: Config = serde_yaml::from_reader(cfg_file).unwrap();
    let mqtt_client = connect_to_mqtt(&config.mqtt).unwrap();

    let mut threads = Vec::new();
    config.gpiochip.iter().for_each(|gpiochip| {
        let str_path = gpiochip.path.to_string().clone();
        let chip = Chip::new(str_path).unwrap_or_else(|err| {
            panic!("Could not open GPIO chip: {}; error {}", gpiochip.path, err);
        });

        let pins = gpiochip
            .pins
            .iter()
            .map(|pin| pin.header_pin)
            .collect::<Vec<u32>>();

        let opts = Options::input(pins).edge(EdgeDetect::Both);

        let mut inputs = chip.request_lines(opts).unwrap();

        let str_path = gpiochip.path.to_string().clone();
        threads.push(thread::spawn(move || {
            let event = inputs.read_event().unwrap_or_else(|err| {
                panic!("Could not read event: {}; error {}", str_path, err);
            });
            println!("event: {:?}", event);
        }));
    });

    Ok(())
}
