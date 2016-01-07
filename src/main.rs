extern crate lifx;
extern crate libc;
extern crate hsl;

use lifx::*;
use hsl::HSL;


use std::net::UdpSocket;
use std::os::unix::io::AsRawFd;


fn main() {


    let sock = UdpSocket::bind("0.0.0.0:56700").unwrap();

    let sock_fd = sock.as_raw_fd();
    let broadcast: libc::c_int = 1;
    let ret = unsafe {
        let b_ptr: *const libc::c_int = &broadcast;
        libc::setsockopt(sock_fd, libc::SOL_SOCKET, libc::SO_BROADCAST, b_ptr as *const libc::c_void, std::mem::size_of::<libc::c_int>() as u32) 
    };
   
    //let msg = RawMessage::build(Some(107296920531920), Messages::Get);
    //let msg = RawMessage::build(Some(107296920531920), Messages::SetPower{level: 65535, duration: 0});
    //sock.send_to(&msg.pack(),"10.10.1.119:56700").unwrap();
    
    //let msg = RawMessage::build(None, Messages::Get);
    //sock.send_to(&msg.pack(),"255.255.255.255:56700").unwrap();

    let mgr = NetManager::new(sock);
    mgr.refresh_all();

    loop {
        std::thread::sleep_ms(5000);
        mgr.maintain();
        if let Some(bulb) = mgr.bulb_by_id(62337169322960) {
            println!("{:?}", bulb);
            if let Some(hsbk) = bulb.color {
                let color = HSL {
                    h: (hsbk.hue as f64/ 65535f64) * 360f64,
                    s: hsbk.saturation as f64/ 65535f64,
                    l: hsbk.brightness as f64/ 65535f64
                };
                //println!("{:?} --> {:?}", color, color.to_rgb());
            }
        }


    }



}
