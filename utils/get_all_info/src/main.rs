use get_if_addrs::{get_if_addrs, IfAddr, Ifv4Addr};
use lifx_core::{get_product_info, BuildOptions, Message, PowerLevel, RawMessage, Service, HSBK};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

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

struct BulbInfo {
    last_seen: Instant,
    source: u32,
    target: u64,
    addr: SocketAddr,
    name: RefreshableData<String>,
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
    fn new(source: u32, target: u64, addr: SocketAddr) -> BulbInfo {
        BulbInfo {
            last_seen: Instant::now(),
            source,
            target,
            addr,
            name: RefreshableData::empty(HOUR, Message::GetLabel),
            model: RefreshableData::empty(HOUR, Message::GetVersion),
            location: RefreshableData::empty(HOUR, Message::GetLocation),
            host_firmware: RefreshableData::empty(HOUR, Message::GetHostFirmware),
            wifi_firmware: RefreshableData::empty(HOUR, Message::GetWifiFirmware),
            power_level: RefreshableData::empty(Duration::from_secs(15), Message::GetPower),
            color: Color::Unknown,
        }
    }

    fn update(&mut self, addr: SocketAddr) {
        self.last_seen = Instant::now();
        self.addr = addr;
    }

    fn refresh_if_needed<T>(
        &self,
        sock: &UdpSocket,
        data: &RefreshableData<T>,
    ) -> Result<(), failure::Error> {
        if data.needs_refresh() {
            let options = BuildOptions {
                target: Some(self.target),
                res_required: true,
                source: self.source,
                ..Default::default()
            };
            let message = RawMessage::build(&options, data.refresh_msg.clone())?;
            sock.send_to(&message.pack()?, self.addr)?;
        }
        Ok(())
    }

    fn query_for_missing_info(&self, sock: &UdpSocket) -> Result<(), failure::Error> {
        self.refresh_if_needed(sock, &self.name)?;
        self.refresh_if_needed(sock, &self.model)?;
        self.refresh_if_needed(sock, &self.location)?;
        self.refresh_if_needed(sock, &self.host_firmware)?;
        self.refresh_if_needed(sock, &self.wifi_firmware)?;
        self.refresh_if_needed(sock, &self.power_level)?;
        match &self.color {
            Color::Unknown => (), // we'll need to wait to get info about this bulb's model, so we'll know if it's multizone or not
            Color::Single(d) => self.refresh_if_needed(sock, d)?,
            Color::Multi(d) => self.refresh_if_needed(sock, d)?,
        }

        Ok(())
    }
}

impl std::fmt::Debug for BulbInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BulbInfo({:0>16X} - {}  ", self.target, self.addr)?;

        if let Some(name) = self.name.as_ref() {
            write!(f, "{}", name)?;
        }
        if let Some(location) = self.location.as_ref() {
            write!(f, "/{}", location)?;
        }
        if let Some((vendor, product)) = self.model.as_ref() {
            if let Some(info) = get_product_info(*vendor, *product) {
                write!(f, " - {} ", info.name)?;
            } else {
                write!(
                    f,
                    " - Unknown model (vendor={}, product={}) ",
                    vendor, product
                )?;
            }
        }
        if let Some(fw_version) = self.host_firmware.as_ref() {
            write!(f, " McuFW:{:x}", fw_version)?;
        }
        if let Some(fw_version) = self.wifi_firmware.as_ref() {
            write!(f, " WifiFW:{:x}", fw_version)?;
        }
        if let Some(level) = self.power_level.as_ref() {
            if *level == PowerLevel::Enabled {
                write!(f, "  Powered On(")?;
                match self.color {
                    Color::Unknown => write!(f, "??")?,
                    Color::Single(ref color) => {
                        f.write_str(
                            &color
                                .as_ref()
                                .map(|c| c.describe(false))
                                .unwrap_or_else(|| "??".to_owned()),
                        )?;
                    }
                    Color::Multi(ref color) => {
                        if let Some(vec) = color.as_ref() {
                            write!(f, "Zones: ")?;
                            for zone in vec {
                                if let Some(color) = zone {
                                    write!(f, "{} ", color.describe(true))?;
                                } else {
                                    write!(f, "?? ")?;
                                }
                            }
                        }
                    }
                }
                write!(f, ")")?;
            } else {
                write!(f, "  Powered Off")?;
            }
        }
        write!(f, ")")
    }
}

struct Manager {
    bulbs: Arc<Mutex<HashMap<u64, BulbInfo>>>,
    last_discovery: Instant,
    sock: UdpSocket,
    source: u32,
}

impl Manager {
    fn new() -> Result<Manager, failure::Error> {
        let sock = UdpSocket::bind("0.0.0.0:56700")?;
        sock.set_broadcast(true)?;

        // spawn a thread that can send to our socket
        let recv_sock = sock.try_clone()?;

        let bulbs = Arc::new(Mutex::new(HashMap::new()));
        let receiver_bulbs = bulbs.clone();
        let source = 0x72757374;

        // spawn a thread that will receive data from our socket and update our internal data structures
        spawn(move || Self::worker(recv_sock, source, receiver_bulbs));

        let mut mgr = Manager {
            bulbs,
            last_discovery: Instant::now(),
            sock,
            source,
        };
        mgr.discover()?;
        Ok(mgr)
    }

    fn handle_message(raw: RawMessage, bulb: &mut BulbInfo) -> Result<(), lifx_core::Error> {
        match Message::from_raw(&raw)? {
            Message::StateService { port, service } => {
                if port != bulb.addr.port() as u32 || service != Service::UDP {
                    println!("Unsupported service: {:?}/{}", service, port);
                }
            }
            Message::StateLabel { label } => bulb.name.update(label.0),
            Message::StateLocation { label, .. } => bulb.location.update(label.0),
            Message::StateVersion {
                vendor, product, ..
            } => {
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
            Message::StatePower { level } => bulb.power_level.update(level),
            Message::StateHostFirmware { version, .. } => bulb.host_firmware.update(version),
            Message::StateWifiFirmware { version, .. } => bulb.wifi_firmware.update(version),
            Message::LightState {
                color,
                power,
                label,
                ..
            } => {
                if let Color::Single(ref mut d) = bulb.color {
                    d.update(color);
                    bulb.power_level.update(power);
                }
                bulb.name.update(label.0);
            }
            Message::StateZone {
                count,
                index,
                color,
            } => {
                if let Color::Multi(ref mut d) = bulb.color {
                    d.data.get_or_insert_with(|| {
                        let mut v = Vec::with_capacity(count as usize);
                        v.resize(count as usize, None);
                        assert!(index <= count);
                        v
                    })[index as usize] = Some(color);
                }
            }
            Message::StateMultiZone {
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
            } => {
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
            unknown => {
                println!("Received, but ignored {:?}", unknown);
            }
        }
        Ok(())
    }

    fn worker(
        recv_sock: UdpSocket,
        source: u32,
        receiver_bulbs: Arc<Mutex<HashMap<u64, BulbInfo>>>,
    ) {
        let mut buf = [0; 1024];
        loop {
            match recv_sock.recv_from(&mut buf) {
                Ok((0, addr)) => println!("Received a zero-byte datagram from {:?}", addr),
                Ok((nbytes, addr)) => match RawMessage::unpack(&buf[0..nbytes]) {
                    Ok(raw) => {
                        if raw.frame_addr.target == 0 {
                            continue;
                        }
                        if let Ok(mut bulbs) = receiver_bulbs.lock() {
                            let bulb = bulbs
                                .entry(raw.frame_addr.target)
                                .and_modify(|bulb| bulb.update(addr))
                                .or_insert_with(|| {
                                    BulbInfo::new(source, raw.frame_addr.target, addr)
                                });
                            if let Err(e) = Self::handle_message(raw, bulb) {
                                println!("Error handling message from {}: {}", addr, e)
                            }
                        }
                    }
                    Err(e) => println!("Error unpacking raw message from {}: {}", addr, e),
                },
                Err(e) => panic!("recv_from err {:?}", e),
            }
        }
    }

    fn discover(&mut self) -> Result<(), failure::Error> {
        println!("Doing discovery");

        let opts = BuildOptions {
            source: self.source,
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
                    let addr = SocketAddr::new(IpAddr::V4(bcast), 56700);
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
                println!("{:?}", bulb);
            }
        }

        sleep(Duration::from_secs(5));
    }
}
