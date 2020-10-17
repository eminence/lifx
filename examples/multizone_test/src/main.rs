use lifx_core::{ApplicationRequest, BuildOptions, Message, RawMessage, HSBK};
use std::net::{SocketAddr, UdpSocket};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    let sock = UdpSocket::bind("0.0.0.0:56700").unwrap();
    sock.set_broadcast(true).unwrap();

    // 0000562B29D573D0 - 10.10.1.131:56700

    let target: SocketAddr = "10.10.1.131:56700".parse().unwrap();

    let msg = Message::SetColorZones {
        start_index: 0,
        end_index: 16,
        color: HSBK {
            hue: 0,
            brightness: 0,
            kelvin: 9000,
            saturation: 0,
        },
        duration: 0,
        apply: ApplicationRequest::Apply,
    };

    let opts = BuildOptions {
        target: Some(0x0000562B29D573D0),
        source: 12345678,
        ..Default::default()
    };

    let raw = RawMessage::build(
        &opts,
        Message::LightSetPower {
            level: 65535,
            duration: 0,
        },
    )
    .unwrap();
    sock.send_to(&raw.pack().unwrap(), &target).unwrap();

    let raw = RawMessage::build(&opts, msg).unwrap();
    sock.send_to(&raw.pack().unwrap(), &target).unwrap();

    let duration = 50;

    loop {
        for idx in 0..16 {
            // turn on this zone, and turn off the previous one (if applicable)
            let msg = Message::SetColorZones {
                start_index: idx,
                end_index: idx,
                color: HSBK {
                    hue: 0,
                    brightness: 65535,
                    kelvin: 3000,
                    saturation: 65535,
                },
                duration,
                apply: ApplicationRequest::Apply,
            };

            let raw = RawMessage::build(&opts, msg).unwrap();
            sock.send_to(&raw.pack().unwrap(), &target).unwrap();

            if idx > 0 {
                let msg = Message::SetColorZones {
                    start_index: idx - 1,
                    end_index: idx - 1,
                    color: HSBK {
                        hue: 0,
                        brightness: 0,
                        kelvin: 3000,
                        saturation: 65535,
                    },
                    duration,
                    apply: ApplicationRequest::Apply,
                };

                let raw = RawMessage::build(&opts, msg).unwrap();
                sock.send_to(&raw.pack().unwrap(), &target).unwrap();
            }

            sleep(Duration::from_millis(duration as u64));
        }

        for idx in 0..16 {
            let idx = 15 - idx;

            // turn on this zone, and turn off the previous one (if applicable)
            let msg = Message::SetColorZones {
                start_index: idx,
                end_index: idx,
                color: HSBK {
                    hue: 0,
                    brightness: 65535,
                    kelvin: 3000,
                    saturation: 65535,
                },
                duration,
                apply: ApplicationRequest::Apply,
            };

            let raw = RawMessage::build(&opts, msg).unwrap();
            sock.send_to(&raw.pack().unwrap(), &target).unwrap();

            if idx < 15 {
                let msg = Message::SetColorZones {
                    start_index: idx + 1,
                    end_index: idx + 1,
                    color: HSBK {
                        hue: 0,
                        brightness: 0,
                        kelvin: 3000,
                        saturation: 65535,
                    },
                    duration,
                    apply: ApplicationRequest::Apply,
                };

                let raw = RawMessage::build(&opts, msg).unwrap();
                sock.send_to(&raw.pack().unwrap(), &target).unwrap();
            }

            sleep(Duration::from_millis(duration as u64));
        }
    }
}
