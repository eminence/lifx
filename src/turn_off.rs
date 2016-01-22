extern crate lifx;
extern crate libc;

use lifx::*;
use std::net::UdpSocket;
use std::os::unix::io::AsRawFd;
use std::io::Stdout;

fn main() {
    let sock = UdpSocket::bind("0.0.0.0:56700").unwrap();

    let sock_fd = sock.as_raw_fd();
    let broadcast: libc::c_int = 1;
    let ret = unsafe {
        let b_ptr: *const libc::c_int = &broadcast;
        libc::setsockopt(sock_fd, libc::SOL_SOCKET, libc::SO_BROADCAST, b_ptr as *const libc::c_void, std::mem::size_of::<libc::c_int>() as u32) 
    };


    let mgr = NetManager::new(sock);
    mgr.refresh_all();
    // broadcast a PowerOff to all bulbs, and then loop for a while to make sure that they are
    // in-fact all off
    //mgr.broadcast(Messages::LightSetPower{level: 0, duration: 1000});

    for _ in 0..10 {
        std::thread::sleep_ms(1000);
        mgr.refresh_all();
        for (uid, bulb) in mgr.bulbs() {
            if let Some(false) = bulb.powered {
                // ok
            } else {
                println!("Bulb is still on!");
                mgr.send_msg(&bulb, Messages::LightSetPower{level: 0, duration: 250});
            }
            println!("{}", bulb.name.unwrap_or(LifxString::new("Unknown")));
            println!("  Powered: {}", bulb.powered.unwrap());
            println!("  Color:   {:?}", bulb.color);
            println!("-----------------------------------");
        }
        break;
    }

    std::thread::sleep_ms(2000);
}
