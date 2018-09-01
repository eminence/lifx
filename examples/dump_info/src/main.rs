
extern crate lifx_core;

use std::net::UdpSocket;
use lifx_core::RawMessage;
use lifx_core::BuildOptions;
use lifx_core::Message;
use std::thread::sleep;
use std::time::Duration;
use std::thread::spawn;

fn main() {

    let sock = UdpSocket::bind("0.0.0.0:56700").unwrap();
    sock.set_broadcast(true).unwrap();

    let sock_clone = sock.try_clone().unwrap();

    // spawn a thread that will request data from the bulbs
    let looper = spawn(move || {
        loop {
            let options = BuildOptions {res_required: true, source:123445, ..Default::default()};
            let msg = RawMessage::build(&options, Message::GetService).unwrap();
            sock_clone.send_to(&msg.pack().unwrap(),"255.255.255.255:56700").unwrap();

            let msg = RawMessage::build(&options, Message::GetHostInfo).unwrap();
            sock_clone.send_to(&msg.pack().unwrap(),"255.255.255.255:56700").unwrap();

            let msg = RawMessage::build(&options, Message::LightGet).unwrap();
            sock_clone.send_to(&msg.pack().unwrap(),"255.255.255.255:56700").unwrap();

            sleep(Duration::from_secs(50));

        }
    });

    let mut buf = [0; 1014];

    loop {
        // read data from socket and try to decode it as bulb messages

        match sock.recv(&mut buf) {
            Ok(n) if n > 0 => {
                if let Ok(raw) = RawMessage::unpack(&buf[0..n]) {
                    println!("Have a raw message from {}.  Trying to convert to Message", raw.frame_addr.target);
                    match Message::from_raw(&raw) {
                        Ok(msg) => {
                            println!("{:?}", msg);
                        },
                        Err(e) => {
                            println!("Unable to decode raw message with type {}: {:?}", raw.protocol_header.typ, e);
                        }
                    }

                }
            },
            _ => {
                sleep(Duration::from_secs(1))
            }
        }

    }


}