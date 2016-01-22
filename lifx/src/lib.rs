//!
//! A library for controlling your LIFX bulbs.
//!
//! There are a few levels you can use:
//!
//!  * `RawMessage` is used to speak the low-level LIFX protocol.  You will have to manually
//!  send/receive packets to/from the network.
//!  * `Manager` will keep track of light bulb state, byt you'll still have manage the network
//!  communication.
//!  * `NetManager` will periodly refresh bulb state from the network for you.

extern crate byteorder;
extern crate rand;
extern crate chrono;

use std::io::Read;

use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use rand::{Rand, thread_rng};

mod manager;
pub use manager::{Bulb, Manager, NetManager};

mod termmgr;
pub use termmgr::TermMgr;

pub struct EchoPayload([u8; 64]);

impl std::fmt::Debug for EchoPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "<EchoPayload>")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifxIdent([u8; 16]);

/// Lifx strings are fixed-length (32-bytes)

#[derive(Debug, Clone, PartialEq)]
pub struct LifxString(String);
impl LifxString {
    pub fn new(s: &str) -> LifxString {
        LifxString(s.to_owned())
    }
}

impl std::fmt::Display for LifxString {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}

impl std::cmp::PartialEq<str> for LifxString {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

trait LittleEndianWriter<T> : WriteBytesExt {
    fn write_val(&mut self, v: T);
}

impl<T> LittleEndianWriter<u32> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: u32) {
        self.write_u32::<LittleEndian>(v).unwrap();
    }
}

impl<T> LittleEndianWriter<u16> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: u16) {
        self.write_u16::<LittleEndian>(v).unwrap();
    }
}

impl<T> LittleEndianWriter<u8> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: u8) {
        self.write_u8(v).unwrap();
    }
}

impl<T> LittleEndianWriter<f32> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: f32) {
        self.write_f32::<LittleEndian>(v).unwrap();
    }
}

impl<T> LittleEndianWriter<i16> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: i16) {
        self.write_i16::<LittleEndian>(v).unwrap();
    }
}

impl<T> LittleEndianWriter<u64> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: u64) {
        self.write_u64::<LittleEndian>(v).unwrap();
    }
}

impl<T> LittleEndianWriter<LifxString> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: LifxString) {
        for idx in 0..32 {
            if idx >= v.0.len() {
                self.write_u8(0).unwrap();
            } else {
                self.write_u8(v.0.chars().nth(idx).unwrap() as u8).unwrap();
            }
        }
    }
}

impl<T> LittleEndianWriter<LifxIdent> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: LifxIdent) {
        for idx in 0..16 {
            self.write_u8(v.0[idx]).unwrap();
        }
    }
}

impl<T> LittleEndianWriter<EchoPayload> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: EchoPayload) {
        for idx in 0..64 {
            self.write_u8(v.0[idx]).unwrap();
        }
    }
}

impl<T> LittleEndianWriter<HSBK> for Vec<T>
where Vec<T> : WriteBytesExt {
    fn write_val(&mut self, v: HSBK) {
        self.write_val(v.hue);
        self.write_val(v.saturation);
        self.write_val(v.brightness);
        self.write_val(v.kelvin);
    }
}



trait LittleEndianReader<T> {
    fn read_val<R: Read>(c: &mut R) -> T;
}

impl LittleEndianReader<u8> for u8 {
    fn read_val<R: Read>(c: &mut R) -> Self { c.read_u8().unwrap() }
}
impl LittleEndianReader<u16> for u16 {
    fn read_val<R: Read>(c: &mut R) -> Self { c.read_u16::<LittleEndian>().unwrap() }
}
impl LittleEndianReader<u32> for u32 {
    fn read_val<R: Read>(c: &mut R) -> Self { c.read_u32::<LittleEndian>().unwrap() }
}
impl LittleEndianReader<f32> for f32 {
    fn read_val<R: Read>(c: &mut R) -> Self { c.read_f32::<LittleEndian>().unwrap() }
}
impl LittleEndianReader<u64> for u64 {
    fn read_val<R: Read>(c: &mut R) -> Self { c.read_u64::<LittleEndian>().unwrap() }
}
impl LittleEndianReader<i16> for i16 {
    fn read_val<R: Read>(c: &mut R) -> Self { c.read_i16::<LittleEndian>().unwrap() }
}

impl LittleEndianReader<HSBK> for HSBK {
    fn read_val<R: Read>(c: &mut R) -> Self {
        let hue = u16::read_val(c);
        let sat = u16::read_val(c);
        let bri = u16::read_val(c);
        let kel = u16::read_val(c);
        HSBK{hue: hue, saturation: sat, brightness: bri, kelvin: kel}
    }
}

impl LittleEndianReader<LifxIdent> for LifxIdent {
    fn read_val<R: Read>(c: &mut R) -> Self {
        let mut val = [0; 16];
        for idx in 0..16 {
            val[idx] = u8::read_val(c);
        }
        LifxIdent(val)
    }
}

impl LittleEndianReader<LifxString> for LifxString {
    fn read_val<R: Read>(c: &mut R) -> Self {
        let mut label = String::with_capacity(32);
        for _ in 0..32 {
            let c = u8::read_val(c);
            if c > 0 {
                label.push(c as char);
            }
        }
        LifxString(label)
    }
}

impl LittleEndianReader<EchoPayload> for EchoPayload {
    fn read_val<R: Read>(c: &mut R) -> Self {
        let mut val = [0; 64];
        for idx in 0..64 {
            val[idx] = u8::read_val(c);
        }
        EchoPayload(val)
    }
}


macro_rules! unpack {
    ($msg:ident, $typ:ident, $( $n:ident: $t:ident ),*) => {
        {
        let mut c = Cursor::new(&$msg.payload);
        $(
            let $n = $t::read_val(&mut c);
        )*

        Messages::$typ{
            $(
                $n:$n,
            )*
        }
        }

    };
}

/// Decoded LIFX Messages
///
/// This enum lists all of the LIFX message types known to this library.
///
/// Note that other message types exist, but are not officially documented (and so are not
/// available here).
#[derive(Debug)]
pub enum Messages {

    /// GetService - 2
    /// Sent by a client to acquire responses from all devices on the local network. No payload is
    /// required. Causes the devices to transmit a StateService message.
    GetService,

    /// StateService - 3
    /// Response to GetService message.
    ///
    /// Provides the device Service and port. If the Service is temporarily unavailable, then the
    /// port value will be 0.
    ///
    /// Field   Type
    /// service unsigned 8-bit integer, maps to Service
    /// port    unsigned 32-bit integer
    StateService{port: u32, service: u8},

    /// GetHostInfo - 12
    /// Get Host MCU information. No payload is required. Causes the device to transmit a
    /// StateHostInfo message.
    GetHostInfo,

    /// StateHostInfo - 13
    /// Response to GetHostInfo message.
    ///
    /// Provides host MCU information.
    ///
    /// signal: radio receive signal strength in milliWatts
    /// tx: bytes transmitted since power on
    /// rx: bytes received since power on
    /// Field   Type
    /// signal  32-bit float
    /// tx  unsigned 32-bit integer
    /// rx  unsigned 32-bit integer
    /// reserved    signed 16-bit integer
    StateHostInfo{signal: f32, tx: u32, rx: u32, reserved: i16},

    /// GetHostFirmware - 14
    /// Gets Host MCU firmware information. No payload is required. Causes the device to transmit a
    /// StateHostFirmware message.
    GetHostFirmware,

    /// StateHostFirmware - 15
    /// Response to GetHostFirmware message.
    ///
    /// Provides host firmware information.
    ///
    /// build: firmware build time (absolute time in nanoseconds since epoch)
    /// version: firmware version
    /// Field   Type
    /// build   unsigned 64-bit integer
    /// reserved    unsigned 64-bit integer
    /// version unsigned 32-bit integer
    StateHostFirmware{build: u64, reserved: u64, version: u32},

    /// GetWifiInfo - 16
    /// Get Wifi subsystem information. No payload is required. Causes the device to transmit a
    /// StateWifiInfo message.
    GetWifiInfo,

    /// StateWifiInfo - 17
    /// Response to GetWifiInfo message.
    ///
    /// Provides Wifi subsystem information.
    ///
    /// signal: radio receive signal strength in mw
    /// tx: bytes transmitted since power on
    /// rx: bytes received since power on
    /// Field   Type
    /// signal  32-bit float
    /// tx  unsigned 32-bit integer
    /// rx  unsigned 32-bit integer
    /// reserved    signed 16-bit integer
    StateWifiInfo{signal: f32, tx: u32, rx: u32, reserved: i16},

    /// GetWifiFirmware - 18
    /// Get Wifi subsystem firmware. No payload is required. Causes the device to transmit a
    /// StateWifiFirmware message.
    GetWifiFirmware,

    /// StateWifiFirmware - 19
    /// Response to GetWifiFirmware message.
    ///
    /// Provides Wifi subsystem information.
    ///
    /// build: firmware build time (absolute time in nanoseconds since epoch)
    /// version: firmware version
    /// Field   Type
    /// build   unsigned 64-bit integer
    /// reserved    unsigned 64-bit integer
    /// version unsigned 32-bit integer
    StateWifiFirmware{build: u64, reserved: u64, version: u32},


    /// GetPower - 20
    ///
    /// Get device power level. No payload is required. Causes the device to transmit a StatePower
    /// message
    GetPower,

    /// SetPower - 21
    ///
    /// Set device power level.
    ///
    /// Zero implies standby and non-zero sets a corresponding power draw level. Currently only 0
    /// and 65535 are supported.
    ///
    /// Field   Type
    /// level   unsigned 16-bit integer
    SetPower{level: u16},

    /// StatePower - 22
    ///
    /// Response to GetPower message.
    ///
    /// Provides device power level.
    ///
    /// Field   Type
    /// level   unsigned 16-bit integer
    StatePower{level: u16},


    /// GetLabel - 23
    ///
    /// Get device label. No payload is required. Causes the device to transmit a StateLabel
    /// message.
    GetLabel,


    /// SetLabel - 24
    ///
    /// Set the device label text.
    ///
    /// Field   Type
    /// label   string, size: 32 bytes
    SetLabel{label: LifxString},

    /// StateLabel - 25
    ///
    /// Response to GetLabel message.
    ///
    /// Provides device label.
    ///
    /// Field   Type
    /// label   string, size: 32 bytes
    StateLabel{label: LifxString},

    /// GetVersion - 32
    /// Get the hardware version. No payload is required. Causes the device to transmit a
    /// StateVersion message.
    GetVersion,

    /// StateVersion - 33
    /// Response to GetVersion message.
    ///
    /// Provides the hardware version of the device.
    ///
    /// vendor: vendor ID
    /// product: product ID
    /// version: hardware version
    /// Field   Type
    /// vendor  unsigned 32-bit integer
    /// product unsigned 32-bit integer
    /// version unsigned 32-bit integer
    StateVersion{ vendor: u32, product: u32, version: u32},

    /// GetInfo - 34
    ///
    /// Get run-time information. No payload is required. Causes the device to transmit a StateInfo
    /// message.
    GetInfo,


    /// StateInfo - 35
    ///
    /// Response to GetInfo message.
    ///
    /// Provides run-time information of device.
    ///
    /// time: current time (absolute time in nanoseconds since epoch)
    /// uptime: time since last power on (relative time in nanoseconds)
    /// downtime: last power off period, 5 second accuracy (in nanoseconds)
    /// Field   Type
    /// time    unsigned 64-bit integer
    /// uptime  unsigned 64-bit integer
    /// downtime    unsigned 64-bit integer
    StateInfo{time: u64, uptime: u64, downtime: u64},


    /// Acknowledgement - 45
    ///
    /// Response to any message sent with ack_required set to 1. See message header frame address.
    Acknowledgement,

    /// GetLocation - 48
    ///
    /// Ask the bulb to return its location information. No payload is required. Causes the device
    /// to transmit a StateLocation message.
    GetLocation,

    /// StateLocation - 50
    /// Device location.
    ///
    /// Field   Type
    /// location    byte array, size: 16
    /// label   string, size: 32
    /// updated_at  unsigned 64-bit integer
    StateLocation{location: LifxIdent, label: LifxString, updated_at: u64},

    /// GetGroup - 51
    /// Ask the bulb to return its group membership information.
    /// No payload is required.
    /// Causes the device to transmit a StateGroup message.
    GetGroup,


    /// StateGroup - 53
    /// Device group.
    ///
    /// Field   Type
    /// group   byte array, size: 16
    /// label   string, size: 32
    /// updated_at  unsigned 64-bit integer
    StateGroup{group: LifxIdent, label: LifxString, updated_at: u64},

    /// EchoRequest - 58
    ///
    /// Request an arbitrary payload be echoed back. Causes the device to transmit an EchoResponse
    /// message.
    ///
    /// Field   Type
    /// payload byte array, size: 64 bytes
    EchoRequest{payload: EchoPayload},
    /// EchoResponse - 59
    ///
    /// Response to EchoRequest message.
    ///
    /// Echo response with payload sent in the EchoRequest.
    ///
    /// Field   Type
    /// payload byte array, size: 64 bytes
    EchoResponse{payload: EchoPayload},

    /// Get - 101
    ///
    /// Sent by a client to obtain the light state. No payload required. Causes the device to
    /// transmit a State message.
    LightGet,

    /// SetColor - 102
    ///
    /// Sent by a client to change the light state.
    ///
    /// Field   Type
    /// reserved    unsigned 8-bit integer
    /// color   HSBK
    /// duration    unsigned 32-bit integer
    /// The duration is the color transition time in milliseconds.
    ///
    /// If the Frame Address res_required field is set to one (1) then the device will transmit a
    /// State message.
    LightSetColor{reserved: u8, color: HSBK, duration: u32},

    /// State - 107
    ///
    /// Sent by a device to provide the current light state.
    ///
    /// Field   Type
    /// color   HSBK
    /// reserved    signed 16-bit integer
    /// power   unsigned 16-bit integer
    /// label   string, size: 32 bytes
    /// reserved    unsigned 64-bit integer
    LightState{color: HSBK, reserved: i16, power: u16, label: LifxString, reserved2: u64},

    /// GetPower - 116
    ///
    /// Sent by a client to obtain the power level. No payload required. Causes the device to
    /// transmit a StatePower message.
    LightGetPower,

    /// SetPower - 117
    ///
    /// Sent by a client to change the light power level.
    ///
    /// Field   Type
    /// level   unsigned 16-bit integer
    /// duration    unsigned 32-bit integer
    /// The power level must be either 0 or 65535.
    ///
    /// The duration is the power level transition time in milliseconds.
    ///
    /// If the Frame Address res_required field is set to one (1) then the device will transmit a
    /// StatePower message.
    LightSetPower{level: u16, duration: u32},

    /// StatePower - 118
    ///
    /// Sent by a device to provide the current power level.
    ///
    /// Field   Type
    /// level   unsigned 16-bit integer
    LightStatePower{level: u16},

}


impl Messages {
    pub fn get_num(&self) -> u16 {
        match self {
            &Messages::GetService => 2,
            &Messages::StateService{..} => 3,
            &Messages::GetHostInfo => 12,
            &Messages::StateHostInfo{..} => 13,
            &Messages::GetHostFirmware => 14,
            &Messages::StateHostFirmware{..} => 15,
            &Messages::GetWifiInfo => 16,
            &Messages::StateWifiInfo{..} => 17,
            &Messages::GetWifiFirmware => 18,
            &Messages::StateWifiFirmware{..} => 19,
            &Messages::GetPower => 20,
            &Messages::SetPower{..} => 21,
            &Messages::StatePower{..} => 22,
            &Messages::GetLabel => 23,
            &Messages::SetLabel{..} => 24,
            &Messages::StateLabel{..} => 25,
            &Messages::GetVersion => 32,
            &Messages::StateVersion{..} => 33,
            &Messages::GetInfo => 34,
            &Messages::StateInfo{..} => 35,
            &Messages::Acknowledgement => 45,
            &Messages::GetLocation => 48,
            &Messages::StateLocation{..} => 50,
            &Messages::GetGroup => 51,
            &Messages::StateGroup{..} => 53,
            &Messages::EchoRequest{..} => 58,
            &Messages::EchoResponse{..} => 59,
            &Messages::LightGet => 101,
            &Messages::LightSetColor{..} => 102,
            &Messages::LightState{..} => 107,
            &Messages::LightGetPower => 116,
            &Messages::LightSetPower{..} => 117,
            &Messages::LightStatePower{..} => 118
        }
    }

    pub fn from_raw(msg: &RawMessage) -> Option<Messages> {
        use std::io::Cursor;
        match msg.protocol_header.typ {
            2 => Some(Messages::GetService),
            3 => {
                Some(unpack!(msg, StateService, 
                             service:u8,
                             port:u32))
            }
            12 => Some(Messages::GetHostInfo),
            13 => {
                Some(unpack!(msg, StateHostInfo,
                             signal: f32,
                             tx: u32,
                             rx: u32,
                             reserved: i16))
            }
            14 => Some(Messages::GetHostFirmware),
            15 => {
                Some(unpack!(msg, StateHostFirmware,
                             build: u64,
                             reserved: u64,
                             version: u32))
            }
            16 => Some(Messages::GetWifiInfo),
            17 => {
                Some(unpack!(msg, StateWifiInfo,
                             signal: f32,
                             tx: u32,
                             rx: u32,
                             reserved: i16))
            }
            18 => Some(Messages::GetWifiFirmware),
            19 => {
                Some(unpack!(msg, StateWifiFirmware,
                             build: u64,
                             reserved: u64,
                             version: u32))
            }
            20 => Some(Messages::GetPower),
            32 => Some(Messages::GetVersion),
            33 => {
                Some(unpack!(msg, StateVersion,
                     vendor: u32,
                     product: u32,
                     version: u32))
            }
            45 => Some(Messages::Acknowledgement),
            48 => Some(Messages::GetLocation),
            50 => Some(unpack!(msg, StateLocation,
                               location: LifxIdent,
                               label: LifxString,
                               updated_at: u64)),
            51 => Some(Messages::GetGroup),
            53 => Some(unpack!(msg, StateGroup,
                               group: LifxIdent,
                               label: LifxString, 
                               updated_at: u64)),
            54 => Some(unpack!(msg, StateInfo,
                               time: u64,
                               uptime: u64,
                               downtime: u64)),
            58 => Some(unpack!(msg, EchoRequest,
                               payload: EchoPayload)),
            59 => Some(unpack!(msg, EchoResponse,
                               payload: EchoPayload)),
            101 => Some(Messages::LightGet),
            102 => Some(unpack!(msg, LightSetColor,
                                reserved: u8,
                                color: HSBK,
                                duration: u32)),
            107 => Some(unpack!(msg, LightState,
                             color: HSBK,
                             reserved: i16,
                             power: u16,
                             label: LifxString,
                             reserved2: u64)),
            116 => Some(Messages::LightGetPower),
            117 => Some(unpack!(msg, LightSetPower,
                                level: u16, duration: u32)),
            118 => {
                let mut c = Cursor::new(&msg.payload);
                Some(Messages::LightStatePower{level: u16::read_val(&mut c)})

            }
            _ => { println!("unknown msg: {:?}", msg);
                None}

        }


    }


}

/// Bulb color (Hue-Saturation-Brightness-Kelvin)
///
/// A note on colors:
///
/// Colors are represented as Hue-Saturation-Brightness-Kelvin, or HSBK
///
/// When a light is displaying whites, saturation will be zero, hue will be ignored, and only
/// brightness and kelvin will matter.
///
/// When a light is displaying colors, kelvin is ignored.  At 100% brightness, brightness=65535 and
/// saturation is about 1300.
///
/// As wheel brightness decreses to 50%, saturation rises to 65535, while brightness stays at
/// 65535.
///
/// As wheel brightness decreses to 0%, saturation stays the same while brightness decreases to 0.
#[derive(Debug, Clone, PartialEq)]
pub struct HSBK {
    pub hue: u16,
    pub saturation: u16,
    pub brightness: u16,
    pub kelvin: u16
}


/// The raw message structure
///
/// Contains a low-level protocol info, and is not usually want you want
#[derive(Debug, Clone, PartialEq)]
pub struct RawMessage {
    pub frame: Frame,
    pub frame_addr: FrameAddress,
    pub protocol_header: ProtocolHeader,
    pub payload: Vec<u8>,
    

}

/// The Frame section contains information about the following:
///
/// * Size of the entire message
/// * LIFX Protocol number: must be 1024 (decimal)
/// * Use of the Frame Address target field
/// * Source identifier
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// 16 bits: Size of entire message in bytes including this field
    pub size: u16,

    /// 2 bits: Message origin indicator: must be zero (0)
    pub origin: u8,

    /// 1 bit: Determines usage of the Frame Address target field
    pub tagged: bool,

    /// 1 bit: Message includes a target address: must be one (1)
    pub addressable: bool,

    /// 12 bits: Protocol number: must be 1024 (decimal)
    pub protocol: u16,

    /// 32 bits: Source identifier: unique value set by the client, used by responses
    pub source: u32
}

/// The Frame Address section contains the following routing information:
///
/// * Target device address
/// * Acknowledgement message is required flag
/// * State response message is required flag
/// * Message sequence number
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameAddress {
    /// 64 bits: 6 byte device address (MAC address) or zero (0) means all devices
    pub target: u64,

    /// 48 bits: Must all be zero (0)
    pub reserved: [u8; 6],

    /// 6 bits: Reserved
    pub reserved2: u8,

    /// 1 bit: Acknowledgement message required
    pub ack_required: bool,

    /// 1 bit: Response message required
    pub res_required: bool,

    /// 8 bits: Wrap around message sequence number
    pub sequence: u8
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProtocolHeader {

    /// 64 bits: Reserved
    pub reserved: u64,

    /// 16 bits: Message type determines the payload being used
    pub typ: u16,

    /// 16 bits: Reserved
    pub reserved2: u16
}

impl Frame {
    fn packed_size() -> usize { 8 }

    fn validate(&self) {
        assert!(self.origin < 4);
        assert_eq!(self.addressable, true);
        assert_eq!(self.protocol, 1024);
    }
    fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(Self::packed_size());
       
        v.write_u16::<LittleEndian>(self.size).unwrap();

        // pack origin + tagged + addressable +  protocol as a u16
        let mut d: u16 = ((self.origin as u16 & 0b11) << 14) as u16;
        d += if self.tagged { 1 } else { 0 } << 13;
        d += if self.addressable { 1 } else { 0 } << 12;
        d += (self.protocol & 0b111111111111) as u16;

        v.write_u16::<LittleEndian>(d).unwrap();

        v.write_u32::<LittleEndian>(self.source).unwrap();

        v
    }
    fn unpack(v: &[u8]) -> Frame {
        use std::io::Cursor;
        let mut c = Cursor::new(v);

        let size = u16::read_val(&mut c);

        // origin + tagged + addressable + protocol
        let d = u16::read_val(&mut c);

        let origin: u8 =  ((d & 0b1100000000000000) >> 14) as u8;
        let tagged: bool = (d & 0b0010000000000000) > 0;
        let addressable  = (d & 0b0001000000000000) > 0;
        let protocol:u16 =  d & 0b0000111111111111;

        let source = u32::read_val(&mut c);

        let frame = Frame {
            size: size,
            origin: origin,
            tagged: tagged,
            addressable: addressable,
            protocol: protocol,
            source: source };
        frame.validate();
        frame
    }

}

impl FrameAddress {
    fn packed_size() -> usize { 16 }
    fn validate(&self) {
        //assert_eq!(self.reserved, [0;6]);
        //assert_eq!(self.reserved2, 0);
    }
    fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(Self::packed_size());
        v.write_u64::<LittleEndian>(self.target).unwrap();
        for idx in 0..6 {
            v.write_u8(self.reserved[idx]).unwrap();
        }

        let b: u8 = (self.reserved2 << 2) +
            if self.ack_required { 2 } else { 0 } +
                if self.res_required { 1 } else { 0 };
        v.write_u8(b).unwrap();
        v.write_u8(self.sequence);
        v 
    }

    fn unpack(v: &[u8]) -> FrameAddress {
        use std::io::Cursor;
        let mut c = Cursor::new(v);

        let target = u64::read_val(&mut c);

        let mut reserved: [u8; 6] = [0; 6];
        for idx in 0..6 {
            reserved[idx] = u8::read_val(&mut c);
        }

        let b = u8::read_val(&mut c);
        let r: u8 = (b & 0b11111100) >> 2;
        let ack_required = (b & 0b10) > 0;
        let res_required = (b & 0b01) > 0;

        let sequence = u8::read_val(&mut c);

        let f = FrameAddress{
            target: target,
            reserved: reserved,
            reserved2: r,
            ack_required: ack_required, 
            res_required: res_required,
            sequence: sequence
        };
        f.validate();
        f


    }
}

impl ProtocolHeader {
    fn packed_size() -> usize { 12 }
    fn validate(&self) {
        //assert_eq!(self.reserved, 0);
        //assert_eq!(self.reserved2, 0);
    }
    fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(Self::packed_size());
        v.write_u64::<LittleEndian>(self.reserved).unwrap();
        v.write_u16::<LittleEndian>(self.typ).unwrap();
        v.write_u16::<LittleEndian>(self.reserved2).unwrap();
        v
    }
    fn unpack(v: &[u8]) -> ProtocolHeader {
        use std::io::Cursor;
        let mut c = Cursor::new(v);

        let reserved = u64::read_val(&mut c);
        let typ = u16::read_val(&mut c);
        let reserved2 = u16::read_val(&mut c);

        let f = ProtocolHeader {
            reserved: reserved, 
            typ: typ,
            reserved2: reserved2
        };
        f.validate();
        f

    }
}

impl RawMessage {

    /// Build a RawMessage (which is suitable for sending on the network) from a given Message
    /// type.
    ///
    /// If `target` is None, then the message is addressed to all devices.  Else it should be a
    /// bulb UID
    pub fn build(target: Option<u64>, typ: Messages) -> RawMessage {
        let mut rng = thread_rng();


        let frame = Frame {
            size: 0,
            origin: 0,
            tagged: target.is_none(),
            addressable: true,
            protocol: 1024,
            source: u32::rand(&mut rng)
        };
        let addr = FrameAddress {
            target: target.unwrap_or(0),
            reserved: [0;6],
            reserved2: 0,
            ack_required: false,
            res_required: true,
            sequence: 128
        };
        let phead = ProtocolHeader{
            reserved: 0,
            reserved2: 2,
            typ: typ.get_num()
        };

        let mut v = Vec::new();
        match typ {
            Messages::StateService{port, service} => {
                v.write_val(port);
                v.write_val(service);
            }
            Messages::StateHostInfo{signal, tx, rx, reserved} => {
                v.write_val(signal);
                v.write_val(tx);
                v.write_val(rx);
                v.write_val(reserved);
            }
            Messages::StateHostFirmware{build, reserved, version} => {
                v.write_val(build);
                v.write_val(reserved);
                v.write_val(version);
            }
            Messages::StateWifiInfo{signal, tx, rx, reserved} => {
                v.write_val(signal);
                v.write_val(tx);
                v.write_val(rx);
                v.write_val(reserved);
            }
            Messages::StateWifiFirmware{build, reserved, version} => {
                v.write_val(build);
                v.write_val(reserved);
                v.write_val(version);
            }
            Messages::SetPower{level} => {
                v.write_val(level);
            }
            Messages::StatePower{level} => {
                v.write_val(level);
            }
            Messages::SetLabel{label} => {
                v.write_val(label);
            }
            Messages::StateLabel{label} => {
                v.write_val(label);
            }
            Messages::StateVersion{vendor, product, version} => {
                v.write_val(vendor);
                v.write_val(product);
                v.write_val(version);
            }
            Messages::StateInfo{time, uptime, downtime} => {
                v.write_val(time);
                v.write_val(uptime);
                v.write_val(downtime);
            }
            Messages::StateLocation{location, label, updated_at} => {
                v.write_val(location);
                v.write_val(label);
                v.write_val(updated_at);
            }
            Messages::StateGroup{group, label, updated_at} => {
                v.write_val(group);
                v.write_val(label);
                v.write_val(updated_at);
            }
            Messages::EchoRequest{payload} => {
                v.write_val(payload);
            }
            Messages::EchoResponse{payload} => {
                v.write_val(payload);
            }
            Messages::LightSetColor{reserved, color, duration} => {
                v.write_val(reserved);
                v.write_val(color);
                v.write_val(duration);
            }
            Messages::LightState{color, reserved, power, label, reserved2} => {
                v.write_val(color);
                v.write_val(reserved);
                v.write_val(power);
                v.write_val(label);
                v.write_val(reserved2);
            }
            Messages::LightSetPower{level, duration} => {
                v.write_val(if level > 0 { 65535u16 } else { 0u16 });
                v.write_val(duration);
            }
            Messages::LightStatePower{level} => {
                v.write_val(level); 
            }

            _ => ()
        }

        let mut msg = RawMessage {
            frame: frame, 
                frame_addr: addr,
                protocol_header: phead,
                payload: v
        };

        msg.frame.size = msg.packed_size() as u16;

        msg
    }

    // The total size (in bytes) of the packed version of this message.
    pub fn packed_size(&self) -> usize {
        Frame::packed_size() + FrameAddress::packed_size() 
            + ProtocolHeader::packed_size() 
            + self.payload.len()
    }

    /// Validates that this object was constructed correctly.  Panics if not.
    pub fn validate(&self) {
        self.frame.validate();
        self.frame_addr.validate();
        self.protocol_header.validate();
    }

    /// Packs this RawMessage into some bytes that can be send over the network.
    pub fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.packed_size());
        v.extend(self.frame.pack());
        v.extend(self.frame_addr.pack());
        v.extend(self.protocol_header.pack());
        v.extend(&self.payload);
        v
    }
    /// Given some bytes (generally read from a network socket), unpack the data into a
    /// `RawMessage` structure.
    pub fn unpack(v: &[u8]) -> RawMessage {
        let mut start = 0;
        let frame = Frame::unpack(v);
        frame.validate();
        start += Frame::packed_size();
        let addr = FrameAddress::unpack(&v[start..]);
        addr.validate();
        start += FrameAddress::packed_size();
        let proto = ProtocolHeader::unpack(&v[start..]);
        proto.validate();
        start += ProtocolHeader::packed_size();

        let body= Vec::from(&v[start..(frame.size as usize)]);


        RawMessage {
            frame: frame,
            frame_addr: addr,
            protocol_header: proto,
            payload: body,
        }


    }
}


#[test]
fn test_frame() {
    let frame = Frame {
        size: 0x1122,
        origin: 0,
        tagged: true,
        addressable: true,
        protocol: 1024,
        source: 1234567
    };
    frame.validate();

    let v = frame.pack();
    println!("{:?}", v);
    assert_eq!(v[0], 0x22);
    assert_eq!(v[1], 0x11);

    assert_eq!(v.len(), Frame::packed_size());

    let unpacked = Frame::unpack(&v);
    assert_eq!(frame, unpacked);

}

#[test]
fn test_decode_frame() {
    //             00    01    02    03    04    05    06    07
    let v = vec!(0x28, 0x00, 0x00, 0x54, 0x42, 0x52, 0x4b, 0x52);
    let frame = Frame::unpack(&v);
    println!("{:?}", frame);

    // manual decoding:
    // size: 0x0028 ==> 40
    // 0x00, 0x54 (origin, tagged, addressable, protocol)

    //  /-Origin ==> 0
    // || /- addressable=1
    // || | 
    // 01010100 00000000
    //   | 
    //   \- Tagged=0


    assert_eq!(frame.size, 0x0028);
    assert_eq!(frame.origin, 1);
    assert_eq!(frame.addressable, true);
    assert_eq!(frame.tagged, false);
    assert_eq!(frame.protocol, 1024);
    assert_eq!(frame.source, 0x524b5242);
}


#[test]
fn test_decode_frame1() {
    //             00    01    02    03    04    05    06    07
    let v = vec!(0x24, 0x00, 0x00, 0x14, 0xca, 0x41, 0x37, 0x05);
    let frame = Frame::unpack(&v);
    println!("{:?}", frame);

    // 00010100 00000000

    assert_eq!(frame.size, 0x0024);
    assert_eq!(frame.origin, 0);
    assert_eq!(frame.tagged ,false);
    assert_eq!(frame.addressable, true);
    assert_eq!(frame.protocol, 1024);
    assert_eq!(frame.source, 0x053741ca);
    
}


#[test]
fn test_frame_address() {
    let frame = FrameAddress {
        target: 0x11224488,
        reserved: [0; 6],
        reserved2: 0,
        ack_required: true,
        res_required: false,
        sequence: 248
    };
    frame.validate();

    let v = frame.pack();
    assert_eq!(v.len(), FrameAddress::packed_size());
    println!("Packed FrameAddress: {:?}", v);

    let unpacked = FrameAddress::unpack(&v);
    assert_eq!(frame, unpacked);
}

#[test]
fn test_decode_frame_address() {
            //   1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16
    let v = vec!(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x9c);
    assert_eq!(v.len(), FrameAddress::packed_size());

    let frame = FrameAddress::unpack(&v);
    frame.validate();
    println!("FrameAddress: {:?}", frame);
}


#[test]
fn test_protocol_header() {
    let frame = ProtocolHeader {
        reserved: 0,
        reserved2: 0,
        typ: 0x4455
    };
    frame.validate();

    let v = frame.pack();
    assert_eq!(v.len(), ProtocolHeader::packed_size());
    println!("Packed ProtocolHeader: {:?}", v);

    let unpacked = ProtocolHeader::unpack(&v);
    assert_eq!(frame, unpacked);
}

#[test]
fn test_decode_protocol_header() {
            //   1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16
    let v = vec!(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00);
    assert_eq!(v.len(), ProtocolHeader::packed_size());

    let frame = ProtocolHeader::unpack(&v);
    frame.validate();
    println!("ProtocolHeader: {:?}", frame);
}


#[test]
fn test_decode_full() {

    let v = vec!(0x24, 0x00, 0x00, 0x14, 0xca, 0x41, 0x37, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x98, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x33, 0x00, 0x00, 0x00);

    let msg = RawMessage::unpack(&v);
    msg.validate();
    println!("{:#?}", msg);
}



#[test]
fn test_decode_full_1() {

    let v = vec!( 0x58, 0x00, 0x00, 0x54, 0xca, 0x41, 0x37, 0x05, 0xd0, 0x73, 0xd5, 0x02, 0x97, 0xde, 0x00, 0x00, 0x4c, 0x49, 0x46, 0x58, 0x56, 0x32, 0x00, 0xc0, 0x44, 0x30, 0xeb, 0x47, 0xc4, 0x48, 0x18, 0x14, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xb8, 0x0b, 0x00, 0x00, 0xff, 0xff, 0x4b, 0x69, 0x74, 0x63, 0x68, 0x65, 0x6e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00);

    let msg = RawMessage::unpack(&v);
    msg.validate();
    println!("{:#?}", msg);
}



