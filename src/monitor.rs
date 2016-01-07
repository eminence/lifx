extern crate term;
extern crate libc;
extern crate lifx;

use term::terminfo::TerminfoTerminal;
use lifx::*;

use std::net::UdpSocket;
use std::os::unix::io::AsRawFd;
use std::io::Stdout;

/*

Bulbs in: My Home
----------------------------

 #      ###
 #      ###
 # #    ###
 # #    ###
 # #    ### 
 R G B  Bri 

Office 
----------------
State: On




*/



fn main () {
    let sock = UdpSocket::bind("0.0.0.0:56700").unwrap();

    let sock_fd = sock.as_raw_fd();
    let broadcast: libc::c_int = 1;
    let ret = unsafe {
        let b_ptr: *const libc::c_int = &broadcast;
        libc::setsockopt(sock_fd, libc::SOL_SOCKET, libc::SO_BROADCAST, b_ptr as *const libc::c_void, std::mem::size_of::<libc::c_int>() as u32) 
    };

    let mut tmgr = TermMgr::new();

    let mgr = NetManager::new(sock);
    mgr.refresh_all();

    loop {
        std::thread::sleep_ms(1000);
        tmgr.clear();
        mgr.maintain();
        for (uid, bulb) in mgr.bulbs() {
            println!("{}", bulb.name.unwrap_or(LifxString::new("Unknown")));
            println!("  Powered: {}", bulb.powered.unwrap());
            println!("  Color:   {:?}", bulb.color);
        }
    }

}
