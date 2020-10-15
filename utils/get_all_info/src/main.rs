use lifx_core::get_product_info;
use lifx_core::BuildOptions;
use lifx_core::Message;
use lifx_core::RawMessage;

use std::net::UdpSocket;
use std::thread::spawn;
use std::time::Duration;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use chrono::Local;
use lifx_core::PowerLevel;
use lifx_core::HSBK;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

const HOUR: Duration = Duration::from_secs(60 * 60);

#[derive(Debug)]
struct RefreshableData<T> {
    data: Option<T>,
    max_age: Duration,
    last_updated: Instant,
    refresh_msg: Message,
}

impl<T> RefreshableData<T> {
    fn empty(max_age: Duration, refresh_msg: Message) -> RefreshableData<T> {
        RefreshableData {
            data: None,
            max_age,
            last_updated: Instant::now(),
            refresh_msg,
        }
    }
    fn update(&mut self, data: T) {
        self.data = Some(data);
        self.last_updated = Instant::now()
    }
    fn needs_refresh(&self) -> bool {
        self.data.is_none() || self.last_updated.elapsed() > self.max_age
    }
    fn as_ref(&self) -> Option<&T> {
        self.data.as_ref()
    }
}

#[derive(Debug)]
struct BulbInfo {
    last_seen: chrono::DateTime<Local>,
    port: u32,
    target: u64,
    addr: SocketAddr,
    name: RefreshableData<String>,
    /// vendor,product tuple
    model: RefreshableData<(u32, u32)>,
    location: RefreshableData<String>,
    host_firmware: RefreshableData<u32>,
    wifi_firmware: RefreshableData<u32>,
    power_level: RefreshableData<PowerLevel>,
    color: Color,
}

#[derive(Debug)]
enum Color {
    Unknown,
    Single(RefreshableData<HSBK>),
    Multi(RefreshableData<Vec<Option<HSBK>>>),
}

impl BulbInfo {
    fn new(port: u32, target: u64, addr: SocketAddr) -> BulbInfo {
        BulbInfo {
            port,
            target,
            addr,
            name: RefreshableData::empty(HOUR, Message::GetLabel),
            model: RefreshableData::empty(HOUR, Message::GetVersion),
            last_seen: Local::now(),
            location: RefreshableData::empty(HOUR, Message::GetLocation),
            host_firmware: RefreshableData::empty(HOUR, Message::GetHostFirmware),
            wifi_firmware: RefreshableData::empty(HOUR, Message::GetWifiFirmware),
            power_level: RefreshableData::empty(Duration::from_secs(15), Message::GetPower),
            color: Color::Unknown,
        }
    }

    fn query_for_missing_info(&self, sock: &UdpSocket) -> Result<(), failure::Error> {
        let opts = BuildOptions {
            target: Some(self.target),
            res_required: true,
            source: 12345678,
            ..Default::default()
        };

        if self.name.needs_refresh() {
            sock.send_to(
                &RawMessage::build(&opts, self.name.refresh_msg.clone())?.pack()?,
                self.addr,
            )?;
        }
        if self.model.needs_refresh() {
            sock.send_to(
                &RawMessage::build(&opts, self.model.refresh_msg.clone())?.pack()?,
                self.addr,
            )?;
        }
        if self.location.needs_refresh() {
            sock.send_to(
                &RawMessage::build(&opts, self.location.refresh_msg.clone())?.pack()?,
                self.addr,
            )?;
        }
        if self.host_firmware.needs_refresh() {
            sock.send_to(
                &RawMessage::build(&opts, self.host_firmware.refresh_msg.clone())?.pack()?,
                self.addr,
            )?;
        }
        if self.wifi_firmware.needs_refresh() {
            sock.send_to(
                &RawMessage::build(&opts, self.wifi_firmware.refresh_msg.clone())?.pack()?,
                self.addr,
            )?;
        }
        if self.power_level.needs_refresh() {
            sock.send_to(
                &RawMessage::build(&opts, self.power_level.refresh_msg.clone())?.pack()?,
                self.addr,
            )?;
        }

        match self.color {
            Color::Unknown => {
                // we'll need to wait to get info about this bulb's model, so we'll know if it's multizone or not
            }
            Color::Single(ref d) => {
                sock.send_to(
                    &RawMessage::build(&opts, d.refresh_msg.clone())?.pack()?,
                    self.addr,
                )?;
            }
            Color::Multi(ref d) => {
                sock.send_to(
                    &RawMessage::build(&opts, d.refresh_msg.clone())?.pack()?,
                    self.addr,
                )?;
            }
        }

        Ok(())
    }

    fn print(&self) {
        if let Some(name) = self.name.as_ref() {
            if let Some(loc) = self.location.as_ref() {
                print!("{}/{} ({:0>16X} - {})", name, loc, self.target, self.addr);
            } else {
                print!("{} ({:0>16X} - {})", name, self.target, self.addr);
            }
        } else {
            print!("({})", self.target);
        }

        if let Some((vendor, product)) = self.model.as_ref() {
            if let Some(info) = get_product_info(*vendor, *product) {
                println!(" - {}", info.name);
            } else {
                println!(" - Unknown model");
            }
        } else {
            println!();
        }

        if let Some(fw_version) = self.host_firmware.as_ref() {
            print!("  Host FW:{} ", fw_version);
        }
        if let Some(fw_version) = self.wifi_firmware.as_ref() {
            println!("  Wifi FW:{} ", fw_version);
        }
        if let Some(level) = self.power_level.as_ref() {
            if *level == PowerLevel::Enabled {
                print!("  Powered On");
                match self.color {
                    Color::Unknown => {}
                    Color::Single(ref color) => {
                        println!(
                            "  {}",
                            color.as_ref().map_or("".to_owned(), |c| c.describe(false))
                        );
                    }
                    Color::Multi(ref color) => {
                        if let Some(vec) = color.as_ref() {
                            print!("  {} zones: ", vec.len());
                            for zone in vec {
                                if let Some(color) = zone {
                                    print!("{} ", color.describe(true))
                                } else {
                                    print!("?? ");
                                }
                            }
                            println!();
                        } else {
                            println!();
                        }
                    }
                }
            } else {
                println!("  Powered off");
            }
        }
    }
}

struct Manager {
    bulbs: Arc<Mutex<HashMap<u64, BulbInfo>>>,
    last_discovery: Instant,
    sock: UdpSocket,
}

impl Manager {
    fn new() -> Result<Manager, failure::Error> {
        let sock = UdpSocket::bind("0.0.0.0:56700")?;
        sock.set_broadcast(true)?;

        // spawn a thread that can send to our socket
        let recv_sock = sock.try_clone()?;

        let bulbs = Arc::new(Mutex::new(HashMap::new()));
        let receiver_bulbs = bulbs.clone();

        // spawn a thread that will receive data from our socket and update our internal data structures

        spawn(move || {
            let mut buf = [0; 1024];
            loop {
                match recv_sock.recv_from(&mut buf) {
                    Ok((0, _)) => panic!("Received a zero-byte datagram"),
                    Ok((nbytes, addr)) => match RawMessage::unpack(&buf[0..nbytes]) {
                        Ok(raw) => {
                            if raw.frame_addr.target == 0 {
                                continue;
                            }
                            if let Ok(mut bulbs) = receiver_bulbs.lock() {
                                let bulb = bulbs
                                    .entry(raw.frame_addr.target)
                                    .and_modify(|b: &mut BulbInfo| {
                                        b.port = addr.port() as u32;
                                        b.addr = addr;
                                        b.last_seen = Local::now();
                                    })
                                    .or_insert_with(|| {
                                        BulbInfo::new(
                                            addr.port() as u32,
                                            raw.frame_addr.target,
                                            addr,
                                        )
                                    });

                                match Message::from_raw(&raw) {
                                    Ok(Message::StateService { port, .. }) => {
                                        assert_eq!(port, addr.port() as u32);
                                    }
                                    Ok(Message::StateLabel { label }) => {
                                        bulb.name.update(label.0);
                                    }
                                    Ok(Message::StateLocation { label, .. }) => {
                                        bulb.location.update(label.0);
                                    }
                                    Ok(Message::StateVersion {
                                        vendor, product, ..
                                    }) => {
                                        bulb.model.update((vendor, product));
                                        if let Some(info) = get_product_info(vendor, product) {
                                            if info.multizone {
                                                bulb.color = Color::Multi(RefreshableData::empty(
                                                    Duration::from_secs(15),
                                                    Message::GetColorZones {
                                                        start_index: 0,
                                                        end_index: 255,
                                                    },
                                                ))
                                            } else {
                                                bulb.color = Color::Single(RefreshableData::empty(
                                                    Duration::from_secs(15),
                                                    Message::LightGet,
                                                ))
                                            }
                                        }
                                    }
                                    Ok(Message::StatePower { level }) => {
                                        bulb.power_level.update(level);
                                    }
                                    Ok(Message::StateHostFirmware { version, .. }) => {
                                        bulb.host_firmware.update(version);
                                    }
                                    Ok(Message::StateWifiFirmware { version, .. }) => {
                                        bulb.wifi_firmware.update(version);
                                    }
                                    Ok(Message::LightState {
                                        color,
                                        power,
                                        label,
                                        ..
                                    }) => {
                                        if let Color::Single(ref mut d) = bulb.color {
                                            d.update(color);
                                            bulb.power_level.update(power);
                                        }
                                        bulb.name.update(label.0);
                                    }
                                    Ok(Message::StateZone {
                                        count,
                                        index,
                                        color,
                                    }) => {
                                        if let Color::Multi(ref mut d) = bulb.color {
                                            d.data.get_or_insert_with(|| {
                                                let mut v = Vec::with_capacity(count as usize);
                                                v.resize(count as usize, None);
                                                assert!(index <= count);
                                                v
                                            })[index as usize] = Some(color);
                                        }
                                    }
                                    Ok(Message::StateMultiZone {
                                        count,
                                        index,
                                        color0,
                                        color1,
                                        color2,
                                        color3,
                                        color4,
                                        color5,
                                        color6,
                                        color7,
                                    }) => {
                                        if let Color::Multi(ref mut d) = bulb.color {
                                            let v = d.data.get_or_insert_with(|| {
                                                let mut v = Vec::with_capacity(count as usize);
                                                v.resize(count as usize, None);
                                                assert!(index + 7 <= count);
                                                v
                                            });

                                            v[index as usize + 0] = Some(color0);
                                            v[index as usize + 1] = Some(color1);
                                            v[index as usize + 2] = Some(color2);
                                            v[index as usize + 3] = Some(color3);
                                            v[index as usize + 4] = Some(color4);
                                            v[index as usize + 5] = Some(color5);
                                            v[index as usize + 6] = Some(color6);
                                            v[index as usize + 7] = Some(color7);
                                        }
                                    }
                                    Ok(msg) => {
                                        println!("Received, but ignored {:?}", msg);
                                    }
                                    Err(e) => {
                                        if raw.protocol_header.typ != 3 {
                                            println!(
                                                "Could not decode message of type {}: {:?}",
                                                raw.protocol_header.typ, e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println! {"Error unpacking raw message: {:?}", e}
                        }
                    },
                    Err(e) => panic!("recv_from err {:?}", e),
                }
            }
        });

        let mut mgr = Manager {
            bulbs,
            last_discovery: Instant::now(),
            sock,
        };
        mgr.discover()?;
        Ok(mgr)
    }

    fn discover(&mut self) -> Result<(), failure::Error> {
        use get_if_addrs::*;
        use std::net;

        println!("Doing discovery");

        let opts = BuildOptions {
            ..Default::default()
        };
        let rawmsg = RawMessage::build(&opts, Message::GetService).unwrap();
        let bytes = rawmsg.pack().unwrap();

        for addr in get_if_addrs().unwrap() {
            match addr.addr {
                IfAddr::V4(Ifv4Addr {
                    broadcast: Some(bcast),
                    ..
                }) => {
                    if addr.ip().is_loopback() {
                        continue;
                    }
                    let addr = net::SocketAddr::new(net::IpAddr::V4(bcast), 56700);
                    println!("Discovering bulbs on LAN {:?}", addr);
                    self.sock.send_to(&bytes, &addr)?;
                }
                _ => {}
            }
        }

        self.last_discovery = Instant::now();

        Ok(())
    }

    fn refresh(&self) {
        if let Ok(bulbs) = self.bulbs.lock() {
            for bulb in bulbs.values() {
                bulb.query_for_missing_info(&self.sock).unwrap();
            }
        }
    }
}

fn main() {
    let mut mgr = Manager::new().unwrap();

    loop {
        if Instant::now() - mgr.last_discovery > Duration::from_secs(300) {
            mgr.discover().unwrap();
        }
        mgr.refresh();

        println!("\n\n\n\n");
        if let Ok(bulbs) = mgr.bulbs.lock() {
            for bulb in bulbs.values() {
                bulb.print();
            }
        }

        thread::sleep(Duration::from_secs(5));
    }
}
