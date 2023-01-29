use futures::executor::block_on;
use gpiod::{Chip, EdgeDetect, Options};
use paho_mqtt as mqtt; // using paho-mqtt as a client as it's sponsored by the Eclipse foundation
use serde::Deserialize;
use serde_yaml;
use std::{convert::TryInto, process, thread, time};
use log::{debug, info};

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

fn handle_event(event: &gpiod::LineEvent, gpiochip: &GpioChip, mqtt: &Mqtt, mqtt_client: &mqtt::AsyncClient) {
    let pin = gpiochip
        .pins
        .iter()
        .find(|pin| pin.header_pin == event.line.offset())
        .unwrap();
    let payload = match event.event_type {
        gpiod::EventType::RisingEdge => "1",
        gpiod::EventType::FallingEdge => "0",
        _ => panic!("Unknown event type"),
    };
    let message = mqtt::Message::new(&mqtt.topic, payload, 0);
    mqtt_client.publish(message);
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
            info!("Listening for events on: {}", str_path);
            loop {
                let event = inputs.read_event().unwrap_or_else(|err| {
                    panic!("Could not read event: {}; error {}", str_path, err);
                });
                debug!("event: {}: {:?}", str_path, event);
                handle_event(&event, &gpiochip, &config.mqtt, &mqtt_client);
            }
        }));
    });

    loop {
        thread::sleep(time::Duration::from_secs(3600));
    }
    Ok(())
}
