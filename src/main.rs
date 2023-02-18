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
    line: u32,
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
        info!("Connecting to MQTT broker: {}", mqtt.host);
        client.connect(None).await?;
        Ok::<(), mqtt::Error>(())
    }) {
        error!(
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

struct HeartBeatEvent {}

struct ChannelEvent<'a> {
    move_event: Option<MoveEvent<'a>>,
    heartbeat: Option<HeartBeatEvent>,
}

impl<'a> ChannelEvent<'a> {
    fn new(move_event: MoveEvent<'a>) -> Self {
        ChannelEvent {
            move_event: Some(move_event),
            heartbeat: None,
        }
    }
}

impl<'a> From<MoveEvent<'a>> for ChannelEvent<'a> {
    fn from(move_event: MoveEvent<'a>) -> Self {
        ChannelEvent {
            move_event: Some(move_event),
            heartbeat: None,
        }
    }
}

impl<'a> ChannelEvent<'a> {
    fn new_heartbeat() -> Self {
        ChannelEvent {
            move_event: None,
            heartbeat: Some(HeartBeatEvent {}),
        }
    }
}

impl<'a> From<HeartBeatEvent> for ChannelEvent<'a> {
    fn from(heartbeat: HeartBeatEvent) -> Self {
        ChannelEvent {
            move_event: None,
            heartbeat: Some(heartbeat),
        }
    }
}

fn mqtt_thread(receiver: mpsc::Receiver<ChannelEvent<'_>>, config: &Config) {
    info!("Starting MQTT worker thread...");
    let mqtt_client = connect_to_mqtt(&config.mqtt).unwrap();
    loop {
        let event = receiver.recv().unwrap();
        event.heartbeat.map(|_| {
            let payload = format!("{} heartbeat 1", config.mqtt.topic);
            let message = mqtt::Message::new(&config.mqtt.topic.clone(), payload, 0);
            mqtt_client.publish(message);
        });
        event.move_event.map(|event| {
            let mut payload: String = config.mqtt.topic.to_string();

            let unknown_pin = GpioPin {
                name: "Unknown".to_string(),
                line: 0,
            };
            payload.push_str(" ");
            payload.push_str(
                event.gpiochip.pins[event.line as usize]
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
        });
    }
}

fn gpiod_thread<'a>(gpiochip: &'a GpioChip, sender: mpsc::Sender<ChannelEvent<'a>>) {
    info!("Waiting for events on: {}", gpiochip.path);
    let chip = Chip::new(gpiochip.path.clone()).unwrap_or_else(|err| {
        panic!("Could not open GPIO chip: {}; error {}", gpiochip.path, err);
    });

    let pins = gpiochip
        .pins
        .iter()
        .map(|pin| pin.line)
        .collect::<Vec<u32>>();

    let opts = Options::input(pins).edge(EdgeDetect::Both);

    let mut inputs = chip.request_lines(opts).unwrap();

    loop {
        let event = inputs.read_event().unwrap_or_else(|err| {
            panic!("Could not read event: {}; error {}", gpiochip.path, err);
        });
        debug!("event: {}: {:?}", gpiochip.path, event);
        sender
            .send(ChannelEvent::new(MoveEvent {
                gpiochip,
                line: event.line,
                edge: event.edge,
            }))
            .unwrap_or_else(|err| {
                error!("Could not send event: {}; error {}", gpiochip.path, err);
            })
    }
}

fn heartbeater_thread(sender: mpsc::Sender<ChannelEvent<'_>>) {
    info!("Starting heartbeat thread...");
    loop {
        sender
            .send(ChannelEvent::new_heartbeat())
            .unwrap_or_else(|err| {
                error!("Could not send heartbeat: {}", err);
            });
        std::thread::sleep(time::Duration::from_secs(60));
    }
}

fn main() -> std::io::Result<()> {
    env_logger::init();
    info!("Configuring...");
    let cfg_file = std::fs::File::open("/etc/gpio2mqtt.yaml").unwrap_or_else(|_| {
        panic!("Could not open config file: /etc/gpio2mqtt.yaml");
    });
    let config: Config = serde_yaml::from_reader(cfg_file).unwrap();

    thread::scope(|s| {
        let (sender, receiver) = std::sync::mpsc::channel();
        s.spawn(|_| mqtt_thread(receiver, &config));

        for gpiochip in config.gpiochip.iter() {
            let gpiochip_sender = sender.clone();
            s.spawn(move |_| gpiod_thread(gpiochip, gpiochip_sender));
        }
        let heartbeater_sender = sender.clone();
        s.spawn(|_| heartbeater_thread(heartbeater_sender));
    })
    .unwrap();

    loop {
        std::thread::sleep(time::Duration::from_secs(3600));
    }
    Ok(())
}
