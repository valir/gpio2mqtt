#![deny(elided_lifetimes_in_paths)]
use crossbeam::thread;
use futures::executor::block_on;
use gpiod::{Chip, EdgeDetect, Options};
use log::{debug, error, info};
use paho_mqtt as mqtt; // using paho-mqtt as a client as it's sponsored by the Eclipse foundation
use serde::Deserialize;
use serde_yaml;
use std::{process, sync::mpsc, time};

#[derive(Deserialize, Debug)]
struct GpioPin {
    name: String,
    header_pin: u32,
}

#[derive(Deserialize, Debug)]
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

#[derive(Debug)]
struct MoveEvent<'a> {
    gpiochip: &'a GpioChip,
    line: u8,
    edge: gpiod::Edge,
}

fn mqtt_thread(receiver: mpsc::Receiver<MoveEvent<'_>>, config: &Config) {
    println!("Starting MQTT worker thread...");
    let mqtt_client = connect_to_mqtt(&config.mqtt).unwrap();
    loop {
        let event = receiver.recv().unwrap();
        debug!("event: {:?}", event);
        let unknown_pin = GpioPin {
            name: "Unknown".to_string(),
            header_pin: 0,
        };
        let mut payload: String = config.mqtt.topic.to_string();
        payload.push_str(" ");
        payload.push_str(
            event
                .gpiochip
                .pins
                .iter()
                .find(|pin| pin.header_pin as u8 == event.line)
                .unwrap_or_else(|| &unknown_pin)
                .name
                .to_string()
                .as_str(),
        );
        payload.push_str(" ");
        payload.push_str(
            match event.edge {
                gpiod::Edge::Rising => "1",
                gpiod::Edge::Falling => "0",
            }
            .to_owned()
            .as_str(),
        );
        debug!("payload: {}", payload);
        let message = mqtt::Message::new(&config.mqtt.topic.clone(), payload, 0);
        mqtt_client.publish(message);
    }
}

fn gpiod_thread<'a>(gpiochip: &'a GpioChip, sender: mpsc::Sender<MoveEvent<'a>>) {
    info!("Listening for events on: {}", gpiochip.path);
    let chip = Chip::new(gpiochip.path.clone()).unwrap_or_else(|err| {
        panic!("Could not open GPIO chip: {}; error {}", gpiochip.path, err);
    });

    let pins = gpiochip
        .pins
        .iter()
        .map(|pin| pin.header_pin)
        .collect::<Vec<u32>>();

    let opts = Options::input(pins).edge(EdgeDetect::Both);

    let mut inputs = chip.request_lines(opts).unwrap();

    loop {
        let event = inputs.read_event().unwrap_or_else(|err| {
            panic!("Could not read event: {}; error {}", gpiochip.path, err);
        });
        debug!("event: {}: {:?}", gpiochip.path, event);
        sender
            .send(MoveEvent {
                gpiochip,
                line: event.line,
                edge: event.edge,
            })
            .unwrap_or_else(|err| {
                error!("Could not send event: {}; error {}", gpiochip.path, err);
            })
    }
}

fn main() -> std::io::Result<()> {
    env_logger::init();
    println!("Starting move-detect...");
    let cfg_file = std::fs::File::open("/etc/move-detect.yaml").unwrap_or_else(|_| {
        panic!("Could not open config file: /etc/move-detect.yaml");
    });
    let config: Config = serde_yaml::from_reader(cfg_file).unwrap();

    thread::scope(|s| {
        let (sender, receiver) = std::sync::mpsc::channel();
        s.spawn(|_| mqtt_thread(receiver, &config));

        for gpiochip in config.gpiochip.iter() {
            let gpiochip_sender = sender.clone();
            s.spawn(move |_| gpiod_thread(gpiochip, gpiochip_sender));
        }
    })
    .unwrap();

    loop {
        std::thread::sleep(time::Duration::from_secs(3600));
    }
    Ok(())
}
