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


    let normal_white = HSBK{hue: 0, saturation: 0, brightness: 35535, kelvin: 2750};
    let dim_white = HSBK{hue: 0, saturation: 0, brightness: 100, kelvin: 2750};

    let mgr = NetManager::new(sock);
    mgr.refresh_all(None);
    std::thread::sleep_ms(10000);

    // broadcast a power ON

    mgr.broadcast_sync(Message::LightSetPower{level: 65535, duration: 1000}, 4);
    mgr.broadcast_sync(Message::LightSetColor{color: normal_white, duration: 500, reserved:0}, 4);

    std::thread::sleep_ms(2000);

    mgr.broadcast_sync(Message::LightSetPower{level: 0, duration: 10*60*1000}, 4);

    std::thread::sleep_ms(2000);

    println!("done");

    std::thread::sleep_ms(2000);
}
