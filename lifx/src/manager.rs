
use ::{
    RawMessage,
    Message,
    HSBK,
    LifxString,
    LifxIdent,
    BuildOptions
};

use chrono::datetime::DateTime;
use chrono::offset::local::Local;
use chrono::duration::Duration;

use rand;

use std::num::Wrapping;
use std::thread;
use std::collections::HashMap;
use std::net::{
    UdpSocket,
    SocketAddr
};

use std::sync::{
    Arc,
    Mutex,
    Condvar,
    RwLock
};



/// Represents the state of a LIFX bulb.
///
/// Note that the data stored in this struct is not "live".  
#[derive(Debug, Clone, PartialEq)]
pub struct Bulb {
    pub name: Option<LifxString>,
    pub color: Option<HSBK>,
    pub powered: Option<bool>,
    port: Option<u32>,
    addr: Option<SocketAddr>,
    pub id: u64,
    last_heard: DateTime<Local>,
    group_label: Option<LifxString>,
    location_label: Option<LifxString>,


}

impl Bulb {
    fn default(target: u64) -> Self {
        Bulb {
            name: None,
            color: None,
            powered: None,
            port: None,
            addr: None,
            id: target,
            last_heard: Local::now(),
            group_label: None,
            location_label: None

        }
    }
}

/// Handles network communication for you
pub struct NetManager {
    mgr: Arc<Mutex<Manager>>,
    sock: UdpSocket,
    cvar: Arc<RwLock<HashMap<u8, Arc<(Mutex<u8>, Condvar)>>>>
}

impl NetManager {
    pub fn new(sock: UdpSocket) -> NetManager {

        let _mgr = Arc::new(Mutex::new(Manager::new()));
        let mgr = _mgr.clone();

        // start up a thread to read messages off the net
        let rsock = sock.try_clone().unwrap();
        let cvar : Arc<RwLock<HashMap<u8, Arc<(Mutex<u8>, Condvar)>>>> = Arc::new(RwLock::new(HashMap::new()));
        let self_cvar = cvar.clone();
        let thr = thread::spawn(move || {
            let mut buf = [0;2048];
            loop {
                let (amt, src) = rsock.recv_from(&mut buf).unwrap();
                //println!("Received {}  bytes from {:?}", amt, src);
                let raw = RawMessage::unpack(&buf[0..amt]);
                {
                    mgr.lock().unwrap().update(&raw, src);
                }
                let seq = raw.frame_addr.sequence;
                if let Some(arc) = self_cvar.write().unwrap().get(&seq) {
                    let (ref mutex, ref cvar) = **arc;
                    let mut x = mutex.lock().unwrap();
                    *x += 1;
                    println!("Trying to wait up thread waiting on ack for seq {}", seq);
                    cvar.notify_all();
                }
            }

        });

        NetManager {
            mgr: _mgr,
            sock: sock,
            cvar: cvar
        }
    }


    /// Broadcast the given message.  Not all messages make sense in a broadcast content, so take
    /// care.
    pub fn broadcast(&self, msg: Message) {
        let msg = RawMessage::build(&BuildOptions::default(), msg);
        self.sock.send_to(&msg.pack(),"255.255.255.255:56700").unwrap();
    }

    /// Broadcast a message, and wait for the given number of ACKs
    pub fn broadcast_sync(&self, msg: Message, num_acks: u8) {
        let mut options = BuildOptions::default();
        options.ack_required = true;
        options.sequence = { self.mgr.lock().unwrap().next_seq() };
        let seq = options.sequence;
        let msg = RawMessage::build(&options, msg);
        println!("Sending message to broadcast with seq={}", seq);

        let pair = Arc::new((Mutex::new(0), Condvar::new()));
        let par2 = pair.clone();
        {
            let mut cvar_map = self.cvar.write().unwrap();
            cvar_map.insert(seq, par2);
        }

        self.sock.send_to(&msg.pack(),"255.255.255.255:56700").unwrap();

        let &(ref lock, ref cvar) = &*pair;
        // have_ack is the number of acks we've received
        let mut have_ack = lock.lock().unwrap();
        while *have_ack < num_acks {
                println!("Current acks: {}", *have_ack);
                have_ack = cvar.wait(have_ack).unwrap();
        }
        println!("Ack for {} received", seq);
        
        {
            let mut cvar_map = self.cvar.write().unwrap();
            if let None = cvar_map.remove(&seq) {
                println!("Hmm, unable to remove seq {} from cvar map", seq);
            }
        }


    }

    pub fn send_msg(&self, bulb: &Bulb, msg: Message) {
        let mut options = BuildOptions::default();
        options.target = Some(bulb.id);
        let msg = RawMessage::build(&options, msg);
        println!("Sending message to {:?}", bulb.addr.unwrap());
        self.sock.send_to(&msg.pack(), bulb.addr.unwrap()).unwrap();
    }

    /// Sends a message and waits for it to be ackd by the bulb
    pub fn send_msg_sync(&self, bulb: &Bulb, msg: Message) {
        let mut options = BuildOptions::default();
        options.target = Some(bulb.id);
        options.ack_required = true;
        options.sequence = { self.mgr.lock().unwrap().next_seq() };
        let seq = options.sequence;
        let msg = RawMessage::build(&options, msg);
        println!("Sending message to {:?} with seq={}", bulb.addr.unwrap(), seq);

        let pair = Arc::new((Mutex::new(0), Condvar::new()));
        let par2 = pair.clone();
        {
            let mut cvar_map = self.cvar.write().unwrap();
            cvar_map.insert(seq, par2);
        }
        
        self.sock.send_to(&msg.pack(), bulb.addr.unwrap()).unwrap();

        let &(ref lock, ref cvar) = &*pair;
        // have_ack is the number of acks we've received
        let mut have_ack = lock.lock().unwrap();
        while *have_ack == 0 {
                have_ack = cvar.wait(have_ack).unwrap();
        }
        println!("Ack for {} received", seq);


        {
            let mut cvar_map = self.cvar.write().unwrap();
            if let None = cvar_map.remove(&seq) {
                println!("Hmm, unable to remove seq {} from cvar map", seq);
            }
        }

    }

    /// Broadcasts a `LightGet` message, which causes all bulbs to identify themselves.
    pub fn refresh_all(&self) {
        let msg = RawMessage::build(&BuildOptions::default(), Message::LightGet);
        self.sock.send_to(&msg.pack(),"255.255.255.255:56700").unwrap();
    }

    /// Requests updated info from a bulb.
    ///
    /// Note that since the communication is async, the data may not be immeditally available once
    /// this method returns
    pub fn refresh(&self, bulb: &Bulb) {
        if let Some(ref addr) = bulb.addr {
            let mut options = BuildOptions::default();
            options.target = Some(bulb.id);
            let msg = RawMessage::build(&options, Message::LightGet);
            self.sock.send_to(&msg.pack(), addr).unwrap();
            let msg = RawMessage::build(&options, Message::GetGroup);
            self.sock.send_to(&msg.pack(), addr).unwrap();
            let msg = RawMessage::build(&options, Message::GetLocation);
            self.sock.send_to(&msg.pack(), addr).unwrap();
        }
    }

    /// Does a refresh for any bulbs that were last heard from more than 60 seconds ago
    pub fn maintain(&self) {
        let now = Local::now();
        let onemin = Duration::seconds(20);

        for bulb in self.mgr.lock().unwrap().bulbs.values() {
            if now - bulb.last_heard > onemin {
                //println!("Need to refresh bulb {:?}", bulb);
                self.refresh(bulb);
            }
        }

    }

    /// Dumps to stdout all known bulbs.  Useful for debugging, but otherwise not recommended.
    pub fn print(&self) {
        self.mgr.lock().unwrap().print();
    }

    pub fn bulb_by_name(&self, name: &str) -> Option<Bulb> {
        self.mgr.lock().unwrap().bulb_by_name(name)
    }

    pub fn bulb_by_id(&self, id: u64) -> Option<Bulb> {
        self.mgr.lock().unwrap().bulb_by_id(id)
    }

    pub fn bulbs(&self) -> HashMap<u64, Bulb> {
        self.mgr.lock().unwrap().bulbs()
    }
}


/// Can be used to keep track of light state, so you don't have to query
/// your bulbs each time.
///
/// If you want to be responsible for the network communication, simply
/// pass incoming RawMessage to the update() method.
///
/// See also `lifx::NetManager` which will also manage some of the network communication for you.
pub struct Manager {
    bulbs: HashMap<u64, Bulb>,
    /// If asked for a sequence number, use this one.  Wraps around
    next_seq: Wrapping<u8>,

    //ack_callbacks: HashMap<u8, Box<Fn()>>
}

impl Manager {
    /// Creates a new bulb manager
    ///
    /// Update its state by reading a RawMessage off the network and passing it to the manager's
    /// update() method
    pub fn new() -> Manager {
        let mut rng = rand::thread_rng();
        Manager {
            bulbs: HashMap::new(),
            next_seq: Wrapping(rand::Rand::rand(&mut rng))
        }

    }

    pub fn next_seq(&mut self) -> u8 {
        let Wrapping(v) = self.next_seq;
        self.next_seq = self.next_seq + Wrapping(1);
        v
    }


    /// Updates the internal list of known bulbs with data from the supplied `RawMessage`
    ///
    /// the `addr` parameter should be the sender of this message
    pub fn update(&mut self, raw: &RawMessage, addr: SocketAddr) {
        let target = raw.frame_addr.target;
        if target == 0 { return }

        let now = Local::now();

        let mut bulb = self.bulbs.entry(target).or_insert(Bulb::default(target));
        bulb.addr = Some(addr);

        if let Some(msg) = Message::from_raw(raw) {
            match msg {
                Message::StateService{port, ..} => {
                    bulb.port = Some(port);
                    bulb.last_heard = now;
                },
                Message::LightState{color, power, label, ..} => {
                    bulb.name = Some(label);
                    bulb.powered = Some(power > 0);
                    bulb.color = Some(color);
                    bulb.last_heard = now;
                }
                Message::LightStatePower{level} => {
                    bulb.powered = Some(level > 0);
                    bulb.last_heard = now;
                }
                Message::StateGroup{label, ..} => {
                    bulb.group_label = Some(label);
                }
                Message::StateLocation{label, ..} => {
                    bulb.location_label = Some(label);
                }
                e => {
                    println!("recv: {:?}", e);
                }
            }
        }

    }

    /// Dumps to stdout all known bulbs.  Useful for debugging, but otherwise not recommended.
    pub fn print(&self) {
        println!("Known bulbs:");
        for bulb in self.bulbs.values() {
            println!("{:?}", bulb);
        }
        println!("");
        
    }

    /// Gets info for a bulb by name
    ///
    /// If there are multiple bulbs with the same name, an arbitrary one will be returned.
    /// The Bulb object is a copy of the data.  It will not be updated as the bulb's state is
    /// changed
    pub fn bulb_by_name(&self, name: &str) -> Option<Bulb> {
        for bulb in self.bulbs.values() {
            if let Some(ref n) = bulb.name {
                if n == name { return Some(bulb.clone()) }
            }
        }
        None
    }

    pub fn bulb_by_id(&self, id: u64) -> Option<Bulb> {
        self.bulbs.get(&id).map(|x| x.clone())
    }

    pub fn bulbs(&self) -> HashMap<u64, Bulb> {
        self.bulbs.clone()
    }

}
