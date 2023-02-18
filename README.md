
# gpio2mqtt
A simple and lightweight MQTT to GPIO bridge, written in Rust.

gpio2mqtt allows you to detect changes in GPIO pins and publish them to MQTT.
It has been tested on a BeagleBone Black running Archlinux Arm, but it should
work on any Linux system with a recent kernel, version 5.0 or newer.

## Motivation

I have several BeagleBone boards around my Home Assistant setup, and I
needed a way to detect changes in GPIO pins and publish them to MQTT. I
currently use this to detect when any of the presence sensors in my house is
on or off. But it can be used for anything that requires detecting changes in
an input pin out of a dry contact sensor. It will detect both rising and
falling edges and publish them to MQTT. See the Configuration section below
for more details about this.

# Installation

Currently the only way to install gpio2mqtt is to build it from source.

## Build from source

This program is written in Rust, so you'll first need to install Rust. The
following commands should be run on the target system. For instance, I've run
them on my BeagleBone Black.

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

For more information about the above commands, see the [Rust installation](https://www.rust-lang.org/tools/install) page.

Then you can build the program with cargo:

```
git clone <this repo>
cd gpio2mqtt
cargo build --release
sudo cp target/release/gpio2mqtt /usr/local/bin
```

#H3 Usage

gpio2mqtt is configured using a YAML file. The default location for the YAML
file is `/etc/gpio2mqtt.yaml`. We provide a sample configuration file in this
repository.

```
sudo cp etc/gpio2mqtt.yaml.sample /etc/gpio2mqtt.yaml
```

You can then edit the configuration file to your liking. For more information
about this file, see the [Configuration](#configuration) section.

## Running

gpio2mqtt can be run as a systemd service. We provide a sample systemd service file in this repository.

```
sudo cp etc/systemd/system/gpio2mqtt.service /etc/systemd/system/gpio2mqtt.service
sudo systemctl daemon-reload
sudo systemctl enable gpio2mqtt
sudo systemctl start gpio2mqtt
```

Check if it is running:

```
sudo systemctl status gpio2mqtt
```

When it's up and running, gpio2mqtt will send a regular heartbeat to the MQTT
topic you've configured in the YAML file. You can use this to check if the
service is running. The heartbeat is sent every 60 seconds.

``
mosquitto_sub -h <your MQTT broker> -t <your main topic>/heartbeat
``

# Configuration

gpio2mqtt is configured using a YAML file. The default location for the YAML
is `/etc/gpio2mqtt.yaml`. Once you've created that file from the sample file
above, you can edit it to your liking. This file has the following sections:
- `mqtt`: MQTT configuration
- `gpiochip`: GPIO configuration

## MQTT configuration

The MQTT configuration section has the following options:
- topic: The main topic to use for all messages. This is the topic that will
  be prepended to all messages sent by gpio2mqtt. The default is `gpio2mqtt`.
- host: The MQTT broker to connect to. The default is `localhost`.

## GPIO configuration

The GPIO configuration consists of a maine `gpiochip` section, and one or more
`path` entries, one for each GPIO chip you want to use. These correspond to
the output of the `gpioinfo` command. For example, on my BeagleBone Black, I
get this output:

```
$ gpioinfo
gpiochip0 - 32 lines:
        line   0: "P8_25 [mmc1_dat0]" unused input active-high
        line   1: "[mmc1_dat1]" unused input active-high
        line   2: "P8_5 [mmc1_dat2]" unused input active-high
        line   3: "P8_6 [mmc1_dat3]" unused input active-high
        line   4: "P8_23 [mmc1_dat4]" unused input active-high
        line   5: "P8_22 [mmc1_dat5]" unused input active-high
        line   6: "P8_3 [mmc1_dat6]" unused input active-high
        line   7: "P8_4 [mmc1_dat7]" unused input active-high
        ....... more lines omitted .......
gpiochip1 - 32 lines:
        line   0:     "P9_15B"       unused   input  active-high
        line   1:      "P8_18"       unused   input  active-high
        line   2:       "P8_7"          "?"   input  active-high [used]
        line   3:       "P8_8"          "?"   input  active-high [used]
        line   4:      "P8_10"          "?"   input  active-high [used]
        line   5:       "P8_9"          "?"   input  active-high [used]
        line   6:      "P8_45"       unused   input  active-high
        line   7:      "P8_46"       unused   input  active-high
        line   8:      "P8_43"       unused   input  active-high
        line   9:      "P8_44"       unused   input  active-high
        line  10:      "P8_41"       unused   input  active-high
        line  11:      "P8_42"       unused   input  active-high
        line  12:      "P8_39"       unused   input  active-high
        line  13:      "P8_40"       unused   input  active-high
        line  14:      "P8_37"       unused   input  active-high
        line  15:      "P8_38"       unused   input  active-high
        line  16:      "P8_36"       unused   input  active-high
        line  17:      "P8_34"       unused   input  active-high
        ....... more lines omitted .......
.... more chips omitted ....
```

The first chip is `gpiochip0`, and the second is `gpiochip1`. This output will
help you put together the configuration file. For example, if I want to use
the "P8_7" pin, I would add the following to the YAML file:

```
gpiochip:
  - path: "gpiochip1"
    pins:
      - line: 2
        name: "P8_7"
```

I'd also like to use the P8_10 pin, so now the entry will be:

```
gpiochip:
  - path: "gpiochip1"
    pins:
      - line: 2
        name: "P8_7"
      - line: 4
        name: "P8_10"
```

The `name` field is used to create the MQTT topic payload for the pin. For
instance, if the "P8_10" pin detects an edge from 0 to 1, then the following MQTT message will be sent:

```
gpio2mqtt P8_10 1
```

When the line will change from 1 to 0, the following message will be sent:

```
gpio2mqtt P8_10 0
```

Once you've configured the YAML file, you can start gpio2mqtt. If it's already
running, then use the `restart` command:

```
sudo systemctl restart gpio2mqtt
```

