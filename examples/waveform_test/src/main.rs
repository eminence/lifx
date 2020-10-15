use lifx_core::{BuildOptions, Message, RawMessage, Waveform, HSBK};
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

fn main() {
    let sock = UdpSocket::bind("0.0.0.0:56700").unwrap();
    sock.set_broadcast(true).unwrap();

    //Office/My Home (0000619602D573D0 - 10.10.1.132:56700) - Original 1000

    let target: SocketAddr = "10.10.1.132:56700".parse().unwrap();

    let opts = BuildOptions {
        target: Some(0x0000619602D573D0),
        ack_required: false,
        res_required: false,
        sequence: 0,
        source: 12345678,
    };

    let starting_color = HSBK {
        hue: 0,
        saturation: 0,
        brightness: 65535,
        kelvin: 3500,
    };

    let color = HSBK {
        hue: 2000,
        saturation: 65535,
        brightness: 65535,
        kelvin: 2500,
    };

    let msg = Message::LightSetColor {
        reserved: 0,
        color: starting_color,
        duration: 1000,
    };

    let raw = RawMessage::build(&opts, msg).unwrap();
    let bytes = raw.pack().unwrap();
    sock.send_to(&bytes, &target).unwrap();

    let stdin = std::io::stdin();
    let mut s = String::new();

    println!("When ready, tap the [enter] key with the beat of a song.");
    stdin.read_line(&mut s).unwrap();
    stdin.read_line(&mut s).unwrap();
    let start = Instant::now();
    let mut count = 0;
    for _ in 0..10 {
        stdin.read_line(&mut s).unwrap();
        count += 1;
        let d = start.elapsed();
        println!("{:?}", d / count);
    }
    let period = start.elapsed() / count;

    let msg = Message::SetWaveform {
        reserved: 0,
        transient: true,
        color,
        period: period.as_millis() as u32,
        cycles: 50.0,
        skew_ratio: 20000,
        waveform: Waveform::Saw,
    };

    let raw = RawMessage::build(&opts, msg).unwrap();
    let bytes = raw.pack().unwrap();
    sock.send_to(&bytes, &target).unwrap();
}
