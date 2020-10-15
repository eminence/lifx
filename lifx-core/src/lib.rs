//! This crate provides low-level message types and structures for dealing with the LIFX LAN protocol.
//!
//! This lets you control lights on your local area network.  More info can be found here:
//! https://lan.developer.lifx.com/
//!
//! Since this is a low-level library, it does not deal with issues like talking to the network,
//! caching light state, or waiting for replies.  This should be done at a higher-level library.
//!
//! # Discovery
//!
//! To discover lights on your LAN, send a [Message::GetService] message as a UDP broadcast to port 56700
//! When a device is discovered, the [Service] types and IP port are provided.  To get additional
//! info about each device, send additional Get messages directly to each device (by setting the
//! [FrameAddress::target] field to the bulbs target ID, and then send a UDP packet to the IP address
//! associated with the device).
//!
//! # Reserved fields
//! When *constructing* packets, you must always set every reserved field to zero.  However, it's
//! possible to receive packets with these fields set to non-zero values.  Be conservative in what
//! you send, and liberal in what you accept.
//!
//! # Unknown values
//! It's common to see packets for LIFX bulbs that don't match the documented protocol.  These are
//! suspected to be internal messages that are used by offical LIFX apps, but that aren't documented.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use failure_derive::Fail;
use std::io::Cursor;
use std::{fmt, io};

/// Various message encoding/decoding errors
#[derive(Fail, Debug)]
pub enum Error {
    /// This error means we were unable to parse a raw message because its type is unknown.
    ///
    /// LIFX devices are known to send messages that are not officially documented, so this error
    /// type does not necessarily represent a bug.
    UnknownMessageType(u16),

    /// This error means one of the message fields contains an invalid or unsupported value.
    ///
    /// The inner string is a description of the error.
    ProtocolError(String),
    Io(#[cause] io::Error),
}

impl std::convert::From<io::Error> for Error {
    fn from(io: io::Error) -> Self {
        Error::Io(io)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "An error occurred.")
    }
}

trait LifxFrom<T>: Sized {
    fn from(val: T) -> Result<Self, Error>;
}

macro_rules! derive_lifx_from {
{ $( $t:ty ),*} => {
    $(
        impl LifxFrom<$t> for $t {
            fn from(val: $t) -> Result<Self, Error> { Ok(val)}
        }
    )*

}
}

derive_lifx_from! {
    u8, u16, i16, u32, f32, u64, LifxIdent, LifxString, EchoPayload, HSBK
}

impl LifxFrom<u8> for ApplicationRequest {
    fn from(val: u8) -> Result<ApplicationRequest, Error> {
        match val {
            0 => Ok(ApplicationRequest::NoApply),
            1 => Ok(ApplicationRequest::Apply),
            2 => Ok(ApplicationRequest::ApplyOnly),
            x => Err(Error::ProtocolError(format!(
                "Unknown application request {}",
                x
            ))),
        }
    }
}

impl LifxFrom<u8> for Waveform {
    fn from(val: u8) -> Result<Waveform, Error> {
        match val {
            0 => Ok(Waveform::Saw),
            1 => Ok(Waveform::Sine),
            2 => Ok(Waveform::HalfSign),
            3 => Ok(Waveform::Triangle),
            4 => Ok(Waveform::Pulse),
            x => Err(Error::ProtocolError(format!(
                "Unknown waveform value {}",
                x
            ))),
        }
    }
}

impl LifxFrom<u8> for Service {
    fn from(val: u8) -> Result<Service, Error> {
        if val != Service::UDP as u8 {
            Err(Error::ProtocolError(format!(
                "Unknown service value {}",
                val
            )))
        } else {
            Ok(Service::UDP)
        }
    }
}

impl LifxFrom<u16> for PowerLevel {
    fn from(val: u16) -> Result<PowerLevel, Error> {
        match val {
            x if x == PowerLevel::Enabled as u16 => Ok(PowerLevel::Enabled),
            x if x == PowerLevel::Standby as u16 => Ok(PowerLevel::Standby),
            x => Err(Error::ProtocolError(format!("Unknown power level {}", x))),
        }
    }
}

pub struct EchoPayload(pub [u8; 64]);

impl std::clone::Clone for EchoPayload {
    fn clone(&self) -> EchoPayload {
        let mut p = [0; 64];
        p.clone_from_slice(&self.0);
        EchoPayload(p)
    }
}

impl std::fmt::Debug for EchoPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "<EchoPayload>")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifxIdent(pub [u8; 16]);

/// Lifx strings are fixed-length (32-bytes maximum)
#[derive(Debug, Clone, PartialEq)]
pub struct LifxString(pub String);

impl LifxString {
    /// Constructs a new LifxString, truncating to 32 characters.
    pub fn new(s: &str) -> LifxString {
        LifxString(if s.len() > 32 {
            s[..32].to_owned()
        } else {
            s.to_owned()
        })
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

trait LittleEndianWriter<T>: WriteBytesExt {
    fn write_val(&mut self, v: T) -> Result<(), io::Error>;
}

macro_rules! derive_writer {
{ $( $m:ident: $t:ty ),*} => {
    $(
        impl<T: WriteBytesExt> LittleEndianWriter<$t> for T {
            fn write_val(&mut self, v: $t) -> Result<(), io::Error> {
                self . $m ::<LittleEndian>(v)
            }
        }
    )*

}
}

derive_writer! { write_u32: u32, write_u16: u16, write_i16: i16, write_u64: u64, write_f32: f32 }

impl<T: WriteBytesExt> LittleEndianWriter<u8> for T {
    fn write_val(&mut self, v: u8) -> Result<(), io::Error> {
        self.write_u8(v)
    }
}

impl<T: WriteBytesExt> LittleEndianWriter<bool> for T {
    fn write_val(&mut self, v: bool) -> Result<(), io::Error> {
        self.write_u8(if v { 1 } else { 0 })
    }
}

impl<T> LittleEndianWriter<LifxString> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: LifxString) -> Result<(), io::Error> {
        for idx in 0..32 {
            if idx >= v.0.len() {
                self.write_u8(0)?;
            } else {
                self.write_u8(v.0.chars().nth(idx).unwrap() as u8)?;
            }
        }
        Ok(())
    }
}

impl<T> LittleEndianWriter<LifxIdent> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: LifxIdent) -> Result<(), io::Error> {
        for idx in 0..16 {
            self.write_u8(v.0[idx])?;
        }
        Ok(())
    }
}

impl<T> LittleEndianWriter<EchoPayload> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: EchoPayload) -> Result<(), io::Error> {
        for idx in 0..64 {
            self.write_u8(v.0[idx])?;
        }
        Ok(())
    }
}

impl<T> LittleEndianWriter<HSBK> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: HSBK) -> Result<(), io::Error> {
        self.write_val(v.hue)?;
        self.write_val(v.saturation)?;
        self.write_val(v.brightness)?;
        self.write_val(v.kelvin)?;
        Ok(())
    }
}

impl<T> LittleEndianWriter<PowerLevel> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: PowerLevel) -> Result<(), io::Error> {
        self.write_u16::<LittleEndian>(v as u16)
    }
}

impl<T> LittleEndianWriter<ApplicationRequest> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: ApplicationRequest) -> Result<(), io::Error> {
        self.write_u8(v as u8)
    }
}

impl<T> LittleEndianWriter<Waveform> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: Waveform) -> Result<(), io::Error> {
        self.write_u8(v as u8)
    }
}

trait LittleEndianReader<T> {
    fn read_val(&mut self) -> Result<T, io::Error>;
}

macro_rules! derive_reader {
{ $( $m:ident: $t:ty ),*} => {
    $(
        impl<T: ReadBytesExt> LittleEndianReader<$t> for T {
            fn read_val(&mut self) -> Result<$t, io::Error> {
                self . $m ::<LittleEndian>()
            }
        }
    )*

}
}

derive_reader! { read_u32: u32, read_u16: u16, read_i16: i16, read_u64: u64, read_f32: f32 }

impl<R: ReadBytesExt> LittleEndianReader<u8> for R {
    fn read_val(&mut self) -> Result<u8, io::Error> {
        self.read_u8()
    }
}

impl<R: ReadBytesExt> LittleEndianReader<HSBK> for R {
    fn read_val(&mut self) -> Result<HSBK, io::Error> {
        let hue = self.read_val()?;
        let sat = self.read_val()?;
        let bri = self.read_val()?;
        let kel = self.read_val()?;
        Ok(HSBK {
            hue,
            saturation: sat,
            brightness: bri,
            kelvin: kel,
        })
    }
}

impl<R: ReadBytesExt> LittleEndianReader<LifxIdent> for R {
    fn read_val(&mut self) -> Result<LifxIdent, io::Error> {
        let mut val = [0; 16];
        for v in &mut val {
            *v = self.read_val()?;
        }
        Ok(LifxIdent(val))
    }
}

impl<R: ReadBytesExt> LittleEndianReader<LifxString> for R {
    fn read_val(&mut self) -> Result<LifxString, io::Error> {
        let mut label = String::with_capacity(32);
        for _ in 0..32 {
            let c: u8 = self.read_val()?;
            if c > 0 {
                label.push(c as char);
            }
        }
        Ok(LifxString(label))
    }
}

impl<R: ReadBytesExt> LittleEndianReader<EchoPayload> for R {
    fn read_val(&mut self) -> Result<EchoPayload, io::Error> {
        let mut val = [0; 64];
        for v in val.iter_mut() {
            *v = self.read_val()?;
        }
        Ok(EchoPayload(val))
    }
}

macro_rules! unpack {
    ($msg:ident, $typ:ident, $( $n:ident: $t:ident ),*) => {
        {
        let mut c = Cursor::new(&$msg.payload);
        $(
            let $n: $t = c.read_val()?;
        )*

        Message::$typ{
            $(
                $n: LifxFrom::from($n)?,
            )*
        }
        }

    };
}

//trace_macros!(true);
//message_types! {
//    /// GetService - 2
//    ///
//    /// Sent by a client to acquire responses from all devices on the local network.
//    GetService(2, ),
//    /// StateService - 3
//    ///
//    /// Response to GetService message.  Provides the device Service and Port.  If the Service
//    /// is temporarily unavailable, then the port value will be zero
//    StateService(3, {
//        service: Service,
//        port: u32
//    })
//}
//trace_macros!(false);

/// What services are exposed by the device.
///
/// LIFX only documents the UDP service, though bulbs may support other undocumented services.
/// Since these other services are unsupported by the lifx-core library, a message with a non-UDP
/// service cannot be constructed.
#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum Service {
    UDP = 1,
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PowerLevel {
    Standby = 0,
    Enabled = 65535,
}

/// Controls how/when multizone devices apply color changes
///
/// See also [Message::SetColorZones].
#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum ApplicationRequest {
    /// Don't apply the requested changes until a message with Apply or ApplyOnly is sent
    NoApply = 0,
    /// Apply the changes immediately and apply any pending changes
    Apply = 1,
    /// Ignore the requested changes in this message and only apply pending changes
    ApplyOnly = 2,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum Waveform {
    Saw = 0,
    Sine = 1,
    HalfSign = 2,
    Triangle = 3,
    Pulse = 4,
}

/// Decoded LIFX Messages
///
/// This enum lists all of the LIFX message types known to this library.
///
/// Note that other message types exist, but are not officially documented (and so are not
/// available here).
#[derive(Clone, Debug)]
pub enum Message {
    /// GetService - 2
    ///
    /// Sent by a client to acquire responses from all devices on the local network. No payload is
    /// required. Causes the devices to transmit a StateService message.
    GetService,

    /// StateService - 3
    ///
    /// Response to [Message::GetService] message.
    StateService {
        /// Port number of the light.  If the service is temporarily unavailable, then the port value
        /// will be 0.
        port: u32,
        /// unsigned 8-bit integer, maps to `Service`
        service: Service,
    },

    /// GetHostInfo - 12
    ///
    /// Get Host MCU information. No payload is required. Causes the device to transmit a
    /// [Message::StateHostInfo] message.
    GetHostInfo,

    /// StateHostInfo - 13
    ///
    /// Response to [Message::GetHostInfo] message.
    ///
    /// Provides host MCU information.
    StateHostInfo {
        /// radio receive signal strength in miliWatts
        signal: f32,
        /// Bytes transmitted since power on
        tx: u32,
        /// Bytes received since power on
        rx: u32,
        reserved: i16,
    },

    /// GetHostFirmware - 14
    ///
    /// Gets Host MCU firmware information. No payload is required. Causes the device to transmit a
    /// [Message::StateHostFirmware] message.
    GetHostFirmware,

    /// StateHostFirmware - 15
    ///
    /// Response to [Message::GetHostFirmware] message.
    ///
    /// Provides host firmware information.
    StateHostFirmware {
        /// Firmware build time (absolute time in nanoseconds since epoch)
        build: u64,
        reserved: u64,
        /// Firmware version
        version: u32,
    },

    /// GetWifiInfo - 16
    ///
    /// Get Wifi subsystem information. No payload is required. Causes the device to transmit a
    /// [Message::StateWifiInfo] message.
    GetWifiInfo,

    /// StateWifiInfo - 17
    ///
    /// Response to [Message::GetWifiInfo] message.
    ///
    /// Provides Wifi subsystem information.
    StateWifiInfo {
        /// Radio receive signal strength in mw
        signal: f32,
        /// bytes transmitted since power on
        tx: u32,
        /// bytes received since power on
        rx: u32,
        reserved: i16,
    },

    /// GetWifiFirmware - 18
    ///
    /// Get Wifi subsystem firmware. No payload is required. Causes the device to transmit a
    /// [Message::StateWifiFirmware] message.
    GetWifiFirmware,

    /// StateWifiFirmware - 19
    /// \
    /// Response to [Message::GetWifiFirmware] message.
    ///
    /// Provides Wifi subsystem information.
    StateWifiFirmware {
        /// firmware build time (absolute time in nanoseconds since epoch)
        build: u64,
        reserved: u64,
        /// firmware version
        version: u32,
    },

    /// GetPower - 20
    ///
    /// Get device power level. No payload is required. Causes the device to transmit a [Message::StatePower]
    /// message
    GetPower,

    /// SetPower - 21
    ///
    /// Set device power level.
    SetPower {
        /// normally a u16, but only 0 and 65535 are supported.
        ///
        /// Zero implies standby and non-zero sets a corresponding power draw level.
        level: PowerLevel,
    },

    /// StatePower - 22
    ///
    /// Response to [Message::GetPower] message.
    ///
    /// Provides device power level.
    StatePower { level: PowerLevel },

    /// GetLabel - 23
    ///
    /// Get device label. No payload is required. Causes the device to transmit a [Message::StateLabel]
    /// message.
    GetLabel,

    /// SetLabel - 24
    ///
    /// Set the device label text.
    SetLabel { label: LifxString },

    /// StateLabel - 25
    ///
    /// Response to [Message::GetLabel] message.
    ///
    /// Provides device label.
    StateLabel { label: LifxString },

    /// GetVersion - 32
    ///
    /// Get the hardware version. No payload is required. Causes the device to transmit a
    /// [Message::StateVersion] message.
    GetVersion,

    /// StateVersion - 33
    ///
    /// Response to [Message::GetVersion] message.
    ///
    /// Provides the hardware version of the device.
    StateVersion {
        /// vendor ID
        vendor: u32,
        /// product ID
        product: u32,
        /// hardware version
        version: u32,
    },

    /// GetInfo - 34
    ///
    /// Get run-time information. No payload is required. Causes the device to transmit a [Message::StateInfo]
    /// message.
    GetInfo,

    /// StateInfo - 35
    ///
    /// Response to [Message::GetInfo] message.
    ///
    /// Provides run-time information of device.
    StateInfo {
        /// current time (absolute time in nanoseconds since epoch)
        time: u64,
        /// time since last power on (relative time in nanoseconds)
        uptime: u64,
        /// last power off period (5 second accuracy, in nanoseconds)
        downtime: u64,
    },

    /// Acknowledgement - 45
    ///
    /// Response to any message sent with ack_required set to 1. See message header frame address.
    ///
    /// (Note that technically this message has no payload, but the frame sequence number is stored
    /// here for convenience).
    Acknowledgement { seq: u8 },

    /// GetLocation - 48
    ///
    /// Ask the bulb to return its location information. No payload is required. Causes the device
    /// to transmit a [Message::StateLocation] message.
    GetLocation,

    /// SetLocation -- 49
    ///
    /// Set the device location
    SetLocation {
        /// GUID byte array
        location: LifxIdent,
        /// text label for location
        label: LifxString,
        /// UTC timestamp of last label update in nanoseconds
        updated_at: u64,
    },

    /// StateLocation - 50
    ///
    /// Device location.
    StateLocation {
        location: LifxIdent,
        label: LifxString,
        updated_at: u64,
    },

    /// GetGroup - 51
    ///
    /// Ask the bulb to return its group membership information.
    /// No payload is required.
    /// Causes the device to transmit a [Message::StateGroup] message.
    GetGroup,

    /// SetGroup - 52
    ///
    /// Set the device group
    SetGroup {
        group: LifxIdent,
        label: LifxString,
        updated_at: u64,
    },

    /// StateGroup - 53
    ///
    /// Device group.
    StateGroup {
        group: LifxIdent,
        label: LifxString,
        updated_at: u64,
    },

    /// EchoRequest - 58
    ///
    /// Request an arbitrary payload be echoed back. Causes the device to transmit an [Message::EchoResponse]
    /// message.
    EchoRequest { payload: EchoPayload },

    /// EchoResponse - 59
    ///
    /// Response to [Message::EchoRequest] message.
    ///
    /// Echo response with payload sent in the EchoRequest.
    ///
    EchoResponse { payload: EchoPayload },

    /// Get - 101
    ///
    /// Sent by a client to obtain the light state. No payload required. Causes the device to
    /// transmit a [Message::LightState] message.
    LightGet,

    /// SetColor - 102
    ///
    /// Sent by a client to change the light state.
    ///
    /// If the Frame Address res_required field is set to one (1) then the device will transmit a
    /// State message.
    LightSetColor {
        reserved: u8,
        /// Color in HSBK
        color: HSBK,
        /// Color transition time in milliseconds
        duration: u32,
    },

    /// SetWaveform - 103
    ///
    /// Apply an effect to the bulb.
    SetWaveform {
        reserved: u8,
        transient: bool,
        color: HSBK,
        /// Duration of a cycle in milliseconds
        period: u32,
        /// Number of cycles
        cycles: f32,
        /// Waveform Skew, [-32768, 32767] scaled to [0, 1].
        skew_ratio: i16,
        /// Waveform to use for transition.
        waveform: Waveform,
    },

    /// State - 107
    ///
    /// Sent by a device to provide the current light state.
    LightState {
        color: HSBK,
        reserved: i16,
        power: PowerLevel,
        label: LifxString,
        reserved2: u64,
    },

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
    LightSetPower { level: u16, duration: u32 },

    /// StatePower - 118
    ///
    /// Sent by a device to provide the current power level.
    ///
    /// Field   Type
    /// level   unsigned 16-bit integer
    LightStatePower { level: u16 },

    /// SetWaveformOptional - 119
    ///
    /// Apply an effect to the bulb.
    SetWaveformOptional {
        reserved: u8,
        transient: bool,
        color: HSBK,
        /// Duration of a cycle in milliseconds
        period: u32,
        /// Number of cycles
        cycles: f32,

        skew_ratio: i16,
        waveform: Waveform,
        set_hue: bool,
        set_saturation: bool,
        set_brightness: bool,
        set_kelvin: bool,
    },

    /// GetInfrared - 120
    ///
    /// Gets the current maximum power level of the Infraed channel
    LightGetInfrared,

    /// StateInfrared - 121
    ///
    /// Indicates the current maximum setting for the infrared channel.
    LightStateInfrared { brightness: u16 },

    /// SetInfrared -- 122
    ///
    /// Set the current maximum brightness for the infrared channel.
    LightSetInfrared { brightness: u16 },

    /// SetColorZones - 501
    ///
    /// This message is used for changing the color of either a single or multiple zones.
    /// The changes are stored in a buffer and are only applied once a message with either
    /// [ApplicationRequest::Apply] or [ApplicationRequest::ApplyOnly] set.
    SetColorZones {
        start_index: u8,
        end_index: u8,
        color: HSBK,
        duration: u32,
        apply: ApplicationRequest,
    },

    /// GetColorZones - 502
    ///
    /// GetColorZones is used to request the zone colors for a range of zones. The bulb will respond
    /// with either [Message::StateZone] or [Message::StateMultiZone] messages as required to cover
    /// the requested range. The bulb may send state messages that cover more than the requested
    /// zones. Any zones outside the requested indexes will still contain valid values at the time
    /// the message was sent.
    GetColorZones { start_index: u8, end_index: u8 },

    /// StateZone - 503

    /// The StateZone message represents the state of a single zone with the `index` field indicating
    /// which zone is represented. The `count` field contains the count of the total number of zones
    /// available on the device.
    StateZone { count: u8, index: u8, color: HSBK },

    /// StateMultiZone - 506
    ///
    /// The StateMultiZone message represents the state of eight consecutive zones in a single message.
    /// As in the StateZone message the `count` field represents the count of the total number of
    /// zones available on the device. In this message the `index` field represents the index of
    /// `color0` and the rest of the colors are the consecutive zones thus the index of the
    /// `color_n` zone will be `index + n`.
    StateMultiZone {
        count: u8,
        index: u8,
        color0: HSBK,
        color1: HSBK,
        color2: HSBK,
        color3: HSBK,
        color4: HSBK,
        color5: HSBK,
        color6: HSBK,
        color7: HSBK,
    },
}

impl Message {
    pub fn get_num(&self) -> u16 {
        match *self {
            Message::GetService => 2,
            Message::StateService { .. } => 3,
            Message::GetHostInfo => 12,
            Message::StateHostInfo { .. } => 13,
            Message::GetHostFirmware => 14,
            Message::StateHostFirmware { .. } => 15,
            Message::GetWifiInfo => 16,
            Message::StateWifiInfo { .. } => 17,
            Message::GetWifiFirmware => 18,
            Message::StateWifiFirmware { .. } => 19,
            Message::GetPower => 20,
            Message::SetPower { .. } => 21,
            Message::StatePower { .. } => 22,
            Message::GetLabel => 23,
            Message::SetLabel { .. } => 24,
            Message::StateLabel { .. } => 25,
            Message::GetVersion => 32,
            Message::StateVersion { .. } => 33,
            Message::GetInfo => 34,
            Message::StateInfo { .. } => 35,
            Message::Acknowledgement { .. } => 45,
            Message::GetLocation => 48,
            Message::SetLocation { .. } => 49,
            Message::StateLocation { .. } => 50,
            Message::GetGroup => 51,
            Message::SetGroup { .. } => 52,
            Message::StateGroup { .. } => 53,
            Message::EchoRequest { .. } => 58,
            Message::EchoResponse { .. } => 59,
            Message::LightGet => 101,
            Message::LightSetColor { .. } => 102,
            Message::SetWaveform { .. } => 103,
            Message::LightState { .. } => 107,
            Message::LightGetPower => 116,
            Message::LightSetPower { .. } => 117,
            Message::LightStatePower { .. } => 118,
            Message::SetWaveformOptional { .. } => 119,
            Message::LightGetInfrared => 120,
            Message::LightStateInfrared { .. } => 121,
            Message::LightSetInfrared { .. } => 122,
            Message::SetColorZones { .. } => 501,
            Message::GetColorZones { .. } => 502,
            Message::StateZone { .. } => 503,
            Message::StateMultiZone { .. } => 506,
        }
    }

    /// Tries to parse the payload in a [RawMessage], based on its message type.
    pub fn from_raw(msg: &RawMessage) -> Result<Message, Error> {
        match msg.protocol_header.typ {
            2 => Ok(Message::GetService),
            3 => Ok(unpack!(msg, StateService, service: u8, port: u32)),
            12 => Ok(Message::GetHostInfo),
            13 => Ok(unpack!(
                msg,
                StateHostInfo,
                signal: f32,
                tx: u32,
                rx: u32,
                reserved: i16
            )),
            14 => Ok(Message::GetHostFirmware),
            15 => Ok(unpack!(
                msg,
                StateHostFirmware,
                build: u64,
                reserved: u64,
                version: u32
            )),
            16 => Ok(Message::GetWifiInfo),
            17 => Ok(unpack!(
                msg,
                StateWifiInfo,
                signal: f32,
                tx: u32,
                rx: u32,
                reserved: i16
            )),
            18 => Ok(Message::GetWifiFirmware),
            19 => Ok(unpack!(
                msg,
                StateWifiFirmware,
                build: u64,
                reserved: u64,
                version: u32
            )),
            20 => Ok(Message::GetPower),
            22 => Ok(unpack!(msg, StatePower, level: u16)),
            23 => Ok(Message::GetLabel),
            25 => Ok(unpack!(msg, StateLabel, label: LifxString)),
            32 => Ok(Message::GetVersion),
            33 => Ok(unpack!(
                msg,
                StateVersion,
                vendor: u32,
                product: u32,
                version: u32
            )),
            35 => Ok(unpack!(
                msg,
                StateInfo,
                time: u64,
                uptime: u64,
                downtime: u64
            )),
            45 => Ok(Message::Acknowledgement {
                seq: msg.frame_addr.sequence,
            }),
            48 => Ok(Message::GetLocation),
            50 => Ok(unpack!(
                msg,
                StateLocation,
                location: LifxIdent,
                label: LifxString,
                updated_at: u64
            )),
            51 => Ok(Message::GetGroup),
            53 => Ok(unpack!(
                msg,
                StateGroup,
                group: LifxIdent,
                label: LifxString,
                updated_at: u64
            )),
            58 => Ok(unpack!(msg, EchoRequest, payload: EchoPayload)),
            59 => Ok(unpack!(msg, EchoResponse, payload: EchoPayload)),
            101 => Ok(Message::LightGet),
            102 => Ok(unpack!(
                msg,
                LightSetColor,
                reserved: u8,
                color: HSBK,
                duration: u32
            )),
            107 => Ok(unpack!(
                msg,
                LightState,
                color: HSBK,
                reserved: i16,
                power: u16,
                label: LifxString,
                reserved2: u64
            )),
            116 => Ok(Message::LightGetPower),
            117 => Ok(unpack!(msg, LightSetPower, level: u16, duration: u32)),
            118 => {
                let mut c = Cursor::new(&msg.payload);
                Ok(Message::LightStatePower {
                    level: c.read_val()?,
                })
            }
            121 => Ok(unpack!(msg, LightStateInfrared, brightness: u16)),
            501 => Ok(unpack!(
                msg,
                SetColorZones,
                start_index: u8,
                end_index: u8,
                color: HSBK,
                duration: u32,
                apply: u8
            )),
            502 => Ok(unpack!(msg, GetColorZones, start_index: u8, end_index: u8)),
            503 => Ok(unpack!(msg, StateZone, count: u8, index: u8, color: HSBK)),
            506 => Ok(unpack!(
                msg,
                StateMultiZone,
                count: u8,
                index: u8,
                color0: HSBK,
                color1: HSBK,
                color2: HSBK,
                color3: HSBK,
                color4: HSBK,
                color5: HSBK,
                color6: HSBK,
                color7: HSBK
            )),
            _ => Err(Error::UnknownMessageType(msg.protocol_header.typ)),
        }
    }
}

/// Bulb color (Hue-Saturation-Brightness-Kelvin)
///
/// # Notes:
///
/// Colors are represented as Hue-Saturation-Brightness-Kelvin, or HSBK
///
/// When a light is displaying whites, saturation will be zero, hue will be ignored, and only
/// brightness and kelvin will matter.
///
/// Normal values for "kelvin" are from 2500 (warm/yellow) to 9000 (cool/blue)
///
/// When a light is displaying colors, kelvin is ignored.
///
/// To display "pure" colors, set saturation to full (65535).
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct HSBK {
    pub hue: u16,
    pub saturation: u16,
    pub brightness: u16,
    pub kelvin: u16,
}

impl HSBK {
    pub fn describe(&self, short: bool) -> String {
        match short {
            true if self.saturation == 0 => format!("{}K", self.kelvin),
            true => format!(
                "{:.0}/{:.0}",
                (self.hue as f32 / 65535.0) * 360.0,
                self.saturation as f32 / 655.35
            ),
            false if self.saturation == 0 => format!(
                "{:.0}% White ({})",
                self.brightness as f32 / 655.35,
                describe_kelvin(self.kelvin)
            ),
            false => format!(
                "{}% hue: {} sat: {}",
                self.brightness as f32 / 655.35,
                self.hue,
                self.saturation
            ),
        }
    }
}

/// Describe (in english words) the color temperature as given in kelvin.
///
/// These descriptions match the values shown in the LIFX mobile app.
pub fn describe_kelvin(k: u16) -> &'static str {
    if k <= 2500 {
        "Ultra Warm"
    } else if k > 2500 && k <= 2700 {
        "Incandescent"
    } else if k > 2700 && k <= 3000 {
        "Warm"
    } else if k > 300 && k <= 3200 {
        "Neutral Warm"
    } else if k > 3200 && k <= 3500 {
        "Neutral"
    } else if k > 3500 && k <= 4000 {
        "Cool"
    } else if k > 400 && k <= 4500 {
        "Cool Daylight"
    } else if k > 4500 && k <= 5000 {
        "Soft Daylight"
    } else if k > 5000 && k <= 5500 {
        "Daylight"
    } else if k > 5500 && k <= 6000 {
        "Noon Daylight"
    } else if k > 6000 && k <= 6500 {
        "Bright Daylight"
    } else if k > 6500 && k <= 7000 {
        "Cloudy Daylight"
    } else if k > 7000 && k <= 7500 {
        "Blue Daylight"
    } else if k > 7500 && k <= 8000 {
        "Blue Overcast"
    } else if k > 8000 && k <= 8500 {
        "Blue Water"
    } else {
        "Blue Ice"
    }
}

impl HSBK {}

/// The raw message structure
///
/// Contains a low-level protocol info.  This is what is sent and received via UDP packets.
///
/// To parse the payload, use [Message::from_raw].
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
///
/// The `tagged` field is a boolean that indicates whether the Frame Address target field is
/// being used to address an individual device or all devices.  If `tagged` is true, then the
/// `target` field should be all zeros.
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

    /// 32 bits: Source identifier: unique value set by the client, used by responses.
    ///
    /// If the source identifier is zero, then the LIFX device may send a broadcast message that can
    /// be received by all clients on the same subnet.
    ///
    /// If this packet is a reply, then this source field will be set to the same value as the client-
    /// sent request packet.
    pub source: u32,
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
    pub sequence: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProtocolHeader {
    /// 64 bits: Reserved
    pub reserved: u64,

    /// 16 bits: Message type determines the payload being used
    pub typ: u16,

    /// 16 bits: Reserved
    pub reserved2: u16,
}

impl Frame {
    /// packed sized, in bytes
    fn packed_size() -> usize {
        8
    }

    fn validate(&self) {
        assert!(self.origin < 4);
        assert_eq!(self.addressable, true);
        assert_eq!(self.protocol, 1024);
    }
    fn pack(&self) -> Result<Vec<u8>, Error> {
        let mut v = Vec::with_capacity(Self::packed_size());

        v.write_u16::<LittleEndian>(self.size)?;

        // pack origin + tagged + addressable +  protocol as a u16
        let mut d: u16 = (<u16 as From<u8>>::from(self.origin) & 0b11) << 14;
        d += if self.tagged { 1 } else { 0 } << 13;
        d += if self.addressable { 1 } else { 0 } << 12;
        d += (self.protocol & 0b1111_1111_1111) as u16;

        v.write_u16::<LittleEndian>(d)?;

        v.write_u32::<LittleEndian>(self.source)?;

        Ok(v)
    }
    fn unpack(v: &[u8]) -> Result<Frame, Error> {
        let mut c = Cursor::new(v);

        let size = c.read_val()?;

        // origin + tagged + addressable + protocol
        let d: u16 = c.read_val()?;

        let origin: u8 = ((d & 0b1100_0000_0000_0000) >> 14) as u8;
        let tagged: bool = (d & 0b0010_0000_0000_0000) > 0;
        let addressable = (d & 0b0001_0000_0000_0000) > 0;
        let protocol: u16 = d & 0b0000_1111_1111_1111;

        if protocol != 1024 {
            return Err(Error::ProtocolError(format!(
                "Unpacked frame had protocol version {}",
                protocol
            )));
        }

        let source = c.read_val()?;

        let frame = Frame {
            size,
            origin,
            tagged,
            addressable,
            protocol,
            source,
        };
        Ok(frame)
    }
}

impl FrameAddress {
    fn packed_size() -> usize {
        16
    }
    fn validate(&self) {
        //assert_eq!(self.reserved, [0;6]);
        //assert_eq!(self.reserved2, 0);
    }
    fn pack(&self) -> Result<Vec<u8>, Error> {
        let mut v = Vec::with_capacity(Self::packed_size());
        v.write_u64::<LittleEndian>(self.target)?;
        for idx in 0..6 {
            v.write_u8(self.reserved[idx])?;
        }

        let b: u8 = (self.reserved2 << 2)
            + if self.ack_required { 2 } else { 0 }
            + if self.res_required { 1 } else { 0 };
        v.write_u8(b)?;
        v.write_u8(self.sequence)?;
        Ok(v)
    }

    fn unpack(v: &[u8]) -> Result<FrameAddress, Error> {
        let mut c = Cursor::new(v);

        let target = c.read_val()?;

        let mut reserved: [u8; 6] = [0; 6];
        for slot in &mut reserved {
            *slot = c.read_val()?;
        }

        let b: u8 = c.read_val()?;
        let reserved2: u8 = (b & 0b1111_1100) >> 2;
        let ack_required = (b & 0b10) > 0;
        let res_required = (b & 0b01) > 0;

        let sequence = c.read_val()?;

        let f = FrameAddress {
            target,
            reserved,
            reserved2,
            ack_required,
            res_required,
            sequence,
        };
        f.validate();
        Ok(f)
    }
}

impl ProtocolHeader {
    fn packed_size() -> usize {
        12
    }
    fn validate(&self) {
        //assert_eq!(self.reserved, 0);
        //assert_eq!(self.reserved2, 0);
    }

    /// Packs this part of the packet into some bytes
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let mut v = Vec::with_capacity(Self::packed_size());
        v.write_u64::<LittleEndian>(self.reserved)?;
        v.write_u16::<LittleEndian>(self.typ)?;
        v.write_u16::<LittleEndian>(self.reserved2)?;
        Ok(v)
    }
    fn unpack(v: &[u8]) -> Result<ProtocolHeader, Error> {
        let mut c = Cursor::new(v);

        let reserved = c.read_val()?;
        let typ = c.read_val()?;
        let reserved2 = c.read_val()?;

        let f = ProtocolHeader {
            reserved,
            typ,
            reserved2,
        };
        f.validate();
        Ok(f)
    }
}

/// Options used to contruct a [RawMessage].
///
/// See also [RawMessage::build].
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// If not `None`, this is the ID of the device you want to address.
    ///
    /// To look up the ID of a device, extract it from the [FrameAddress::target] field when a
    /// device sends a [Message::StateService] message.
    pub target: Option<u64>,
    /// Acknowledgement message required.
    ///
    /// Causes the light to send an [Message::Acknowledgement] message.
    pub ack_required: bool,
    /// Response message required.
    ///
    /// Some message types are sent by clients to get data from a light.  These should always have
    /// `res_required` set to true.
    pub res_required: bool,
    /// A wrap around sequence number.  Optional (can be zero).
    ///
    /// By providing a unique sequence value, the response message will also contain the same
    /// sequence number, allowing a client to distinguish between different messages sent with the
    /// same `source` identifier.
    pub sequence: u8,
    /// A unique client identifier. Optional (can be zero).
    ///
    /// If the source is non-zero, then the LIFX device with send a unicast message to the IP
    /// address/port of the client that sent the originating message.  If zero, then the LIFX
    /// device may send a broadcast message that can be received by all clients on the same sub-net.
    pub source: u32,
}

impl std::default::Default for BuildOptions {
    fn default() -> BuildOptions {
        BuildOptions {
            target: None,
            ack_required: false,
            res_required: false,
            sequence: 0,
            source: 0,
        }
    }
}

impl RawMessage {
    /// Build a RawMessage (which is suitable for sending on the network) from a given Message
    /// type.
    ///
    /// If [BuildOptions::target] is None, then the message is addressed to all devices.  Else it should be a
    /// bulb UID (MAC address)
    pub fn build(options: &BuildOptions, typ: Message) -> Result<RawMessage, Error> {
        let frame = Frame {
            size: 0,
            origin: 0,
            tagged: options.target.is_none(),
            addressable: true,
            protocol: 1024,
            source: options.source,
        };
        let addr = FrameAddress {
            target: options.target.unwrap_or(0),
            reserved: [0; 6],
            reserved2: 0,
            ack_required: options.ack_required,
            res_required: options.res_required,
            sequence: options.sequence,
        };
        let phead = ProtocolHeader {
            reserved: 0,
            reserved2: 0,
            typ: typ.get_num(),
        };

        let mut v = Vec::new();
        match typ {
            Message::GetService
            | Message::GetHostInfo
            | Message::GetHostFirmware
            | Message::GetWifiFirmware
            | Message::GetWifiInfo
            | Message::GetPower
            | Message::GetLabel
            | Message::GetVersion
            | Message::GetInfo
            | Message::Acknowledgement { .. }
            | Message::GetLocation
            | Message::GetGroup
            | Message::LightGet
            | Message::LightGetPower
            | Message::LightGetInfrared => {
                // these types have no payload
            }
            Message::SetColorZones {
                start_index,
                end_index,
                color,
                duration,
                apply,
            } => {
                v.write_val(start_index)?;
                v.write_val(end_index)?;
                v.write_val(color)?;
                v.write_val(duration)?;
                v.write_val(apply)?;
            }
            Message::SetWaveform {
                reserved,
                transient,
                color,
                period,
                cycles,
                skew_ratio,
                waveform,
            } => {
                v.write_val(reserved)?;
                v.write_val(transient)?;
                v.write_val(color)?;
                v.write_val(period)?;
                v.write_val(cycles)?;
                v.write_val(skew_ratio)?;
                v.write_val(waveform)?;
            }
            Message::SetWaveformOptional {
                reserved,
                transient,
                color,
                period,
                cycles,
                skew_ratio,
                waveform,
                set_hue,
                set_saturation,
                set_brightness,
                set_kelvin,
            } => {
                v.write_val(reserved)?;
                v.write_val(transient)?;
                v.write_val(color)?;
                v.write_val(period)?;
                v.write_val(cycles)?;
                v.write_val(skew_ratio)?;
                v.write_val(waveform)?;
                v.write_val(set_hue)?;
                v.write_val(set_saturation)?;
                v.write_val(set_brightness)?;
                v.write_val(set_kelvin)?;
            }
            Message::GetColorZones {
                start_index,
                end_index,
            } => {
                v.write_val(start_index)?;
                v.write_val(end_index)?;
            }
            Message::StateZone {
                count,
                index,
                color,
            } => {
                v.write_val(count)?;
                v.write_val(index)?;
                v.write_val(color)?;
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
                v.write_val(count)?;
                v.write_val(index)?;
                v.write_val(color0)?;
                v.write_val(color1)?;
                v.write_val(color2)?;
                v.write_val(color3)?;
                v.write_val(color4)?;
                v.write_val(color5)?;
                v.write_val(color6)?;
                v.write_val(color7)?;
            }
            Message::LightStateInfrared { brightness } => v.write_val(brightness)?,
            Message::LightSetInfrared { brightness } => v.write_val(brightness)?,
            Message::SetLocation {
                location,
                label,
                updated_at,
            } => {
                v.write_val(location)?;
                v.write_val(label)?;
                v.write_val(updated_at)?;
            }
            Message::SetGroup {
                group,
                label,
                updated_at,
            } => {
                v.write_val(group)?;
                v.write_val(label)?;
                v.write_val(updated_at)?;
            }
            Message::StateService { port, service } => {
                v.write_val(port)?;
                v.write_val(service as u8)?;
            }
            Message::StateHostInfo {
                signal,
                tx,
                rx,
                reserved,
            } => {
                v.write_val(signal)?;
                v.write_val(tx)?;
                v.write_val(rx)?;
                v.write_val(reserved)?;
            }
            Message::StateHostFirmware {
                build,
                reserved,
                version,
            } => {
                v.write_val(build)?;
                v.write_val(reserved)?;
                v.write_val(version)?;
            }
            Message::StateWifiInfo {
                signal,
                tx,
                rx,
                reserved,
            } => {
                v.write_val(signal)?;
                v.write_val(tx)?;
                v.write_val(rx)?;
                v.write_val(reserved)?;
            }
            Message::StateWifiFirmware {
                build,
                reserved,
                version,
            } => {
                v.write_val(build)?;
                v.write_val(reserved)?;
                v.write_val(version)?;
            }
            Message::SetPower { level } => {
                v.write_val(level)?;
            }
            Message::StatePower { level } => {
                v.write_val(level)?;
            }
            Message::SetLabel { label } => {
                v.write_val(label)?;
            }
            Message::StateLabel { label } => {
                v.write_val(label)?;
            }
            Message::StateVersion {
                vendor,
                product,
                version,
            } => {
                v.write_val(vendor)?;
                v.write_val(product)?;
                v.write_val(version)?;
            }
            Message::StateInfo {
                time,
                uptime,
                downtime,
            } => {
                v.write_val(time)?;
                v.write_val(uptime)?;
                v.write_val(downtime)?;
            }
            Message::StateLocation {
                location,
                label,
                updated_at,
            } => {
                v.write_val(location)?;
                v.write_val(label)?;
                v.write_val(updated_at)?;
            }
            Message::StateGroup {
                group,
                label,
                updated_at,
            } => {
                v.write_val(group)?;
                v.write_val(label)?;
                v.write_val(updated_at)?;
            }
            Message::EchoRequest { payload } => {
                v.write_val(payload)?;
            }
            Message::EchoResponse { payload } => {
                v.write_val(payload)?;
            }
            Message::LightSetColor {
                reserved,
                color,
                duration,
            } => {
                v.write_val(reserved)?;
                v.write_val(color)?;
                v.write_val(duration)?;
            }
            Message::LightState {
                color,
                reserved,
                power,
                label,
                reserved2,
            } => {
                v.write_val(color)?;
                v.write_val(reserved)?;
                v.write_val(power)?;
                v.write_val(label)?;
                v.write_val(reserved2)?;
            }
            Message::LightSetPower { level, duration } => {
                v.write_val(if level > 0 { 65535u16 } else { 0u16 })?;
                v.write_val(duration)?;
            }
            Message::LightStatePower { level } => {
                v.write_val(level)?;
            }
        }

        let mut msg = RawMessage {
            frame,
            frame_addr: addr,
            protocol_header: phead,
            payload: v,
        };

        msg.frame.size = msg.packed_size() as u16;

        Ok(msg)
    }

    /// The total size (in bytes) of the packed version of this message.
    pub fn packed_size(&self) -> usize {
        Frame::packed_size()
            + FrameAddress::packed_size()
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
    ///
    /// The length of the returned data will be [RawMessage::packed_size] in size.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let mut v = Vec::with_capacity(self.packed_size());
        v.extend(self.frame.pack()?);
        v.extend(self.frame_addr.pack()?);
        v.extend(self.protocol_header.pack()?);
        v.extend(&self.payload);
        Ok(v)
    }
    /// Given some bytes (generally read from a network socket), unpack the data into a
    /// `RawMessage` structure.
    pub fn unpack(v: &[u8]) -> Result<RawMessage, Error> {
        let mut start = 0;
        let frame = Frame::unpack(v)?;
        frame.validate();
        start += Frame::packed_size();
        let addr = FrameAddress::unpack(&v[start..])?;
        addr.validate();
        start += FrameAddress::packed_size();
        let proto = ProtocolHeader::unpack(&v[start..])?;
        proto.validate();
        start += ProtocolHeader::packed_size();

        let body = Vec::from(&v[start..(frame.size as usize)]);

        Ok(RawMessage {
            frame,
            frame_addr: addr,
            protocol_header: proto,
            payload: body,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ProductInfo {
    pub name: &'static str,
    pub color: bool,
    pub infrared: bool,
    pub multizone: bool,
    pub chain: bool,
}

/// Look up info about what a LIFX product supports.
///
/// You can get the vendor and product IDs from a bulb by receiving a [Message::StateVersion] message
///
/// Data is taken from https://github.com/LIFX/products/blob/master/products.json
#[rustfmt::skip]
pub fn get_product_info(vendor: u32, product: u32) -> Option<&'static ProductInfo> {
    match (vendor, product) {
        (1, 1) => Some(&ProductInfo { name: "Original 1000", color: true, infrared: false, multizone: false, chain: false}),
        (1, 3) => Some(&ProductInfo { name: "Color 650", color: true, infrared: false, multizone: false, chain: false}),
        (1, 10) => Some(&ProductInfo { name: "White 800 (Low Voltage)", color: false, infrared: false, multizone: false, chain: false}),
        (1, 11) => Some(&ProductInfo { name: "White 800 (High Voltage)", color: false, infrared: false, multizone: false, chain: false}),
        (1, 18) => Some(&ProductInfo { name: "White 900 BR30 (Low Voltage)", color: false, infrared: false, multizone: false, chain: false}),
        (1, 20) => Some(&ProductInfo { name: "Color 1000 BR30", color: true, infrared: false, multizone: false, chain: false}),
        (1, 22) => Some(&ProductInfo { name: "Color 1000", color: true, infrared: false, multizone: false, chain: false}),
        (1, 27) => Some(&ProductInfo { name: "LIFX A19", color: true, infrared: false, multizone: false, chain: false}),
        (1, 28) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false}),
        (1, 29) => Some(&ProductInfo { name: "LIFX+ A19", color: true, infrared: true, multizone: false, chain: false}),
        (1, 30) => Some(&ProductInfo { name: "LIFX+ BR30", color: true, infrared: true, multizone: false, chain: false}),
        (1, 31) => Some(&ProductInfo { name: "LIFX Z", color: true, infrared: false, multizone: true, chain: false}),
        (1, 32) => Some(&ProductInfo { name: "LIFX Z 2", color: true, infrared: false, multizone: true, chain: false}),
        (1, 36) => Some(&ProductInfo { name: "LIFX Downlight", color: true, infrared: false, multizone: false, chain: false}),
        (1, 37) => Some(&ProductInfo { name: "LIFX Downlight", color: true, infrared: false, multizone: false, chain: false}),
        (1, 38) => Some(&ProductInfo { name: "LIFX Beam", color: true, infrared: false, multizone: true, chain: false}),
        (1, 43) => Some(&ProductInfo { name: "LIFX A19", color: true, infrared: false, multizone: false, chain: false}),
        (1, 44) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false}),
        (1, 45) => Some(&ProductInfo { name: "LIFX+ A19", color: true, infrared: true, multizone: false, chain: false}),
        (1, 46) => Some(&ProductInfo { name: "LIFX+ BR30", color: true, infrared: true, multizone: false, chain: false}),
        (1, 49) => Some(&ProductInfo { name: "LIFX Mini", color: true, infrared: false, multizone: false, chain: false}),
        (1, 50) => Some(&ProductInfo { name: "LIFX Mini Day and Dusk", color: false, infrared: false, multizone: false, chain: false}),
        (1, 51) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false}),
        (1, 52) => Some(&ProductInfo { name: "LIFX GU10", color: true, infrared: false, multizone: false, chain: false}),
        (1, 55) => Some(&ProductInfo { name: "LIFX Tile", color: true, infrared: false, multizone: false, chain: true}),
        (1, 59) => Some(&ProductInfo { name: "LIFX Mini Color", color: true, infrared: false, multizone: false, chain: false}),
        (1, 60) => Some(&ProductInfo { name: "LIFX Mini Day and Dusk", color: false, infrared: false, multizone: false, chain: false}),
        (1, 61) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false}),
        (_, _) => None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame() {
        let frame = Frame {
            size: 0x1122,
            origin: 0,
            tagged: true,
            addressable: true,
            protocol: 1024,
            source: 1234567,
        };
        frame.validate();

        let v = frame.pack().unwrap();
        println!("{:?}", v);
        assert_eq!(v[0], 0x22);
        assert_eq!(v[1], 0x11);

        assert_eq!(v.len(), Frame::packed_size());

        let unpacked = Frame::unpack(&v).unwrap();
        assert_eq!(frame, unpacked);
    }

    #[test]
    fn test_decode_frame() {
        //             00    01    02    03    04    05    06    07
        let v = vec![0x28, 0x00, 0x00, 0x54, 0x42, 0x52, 0x4b, 0x52];
        let frame = Frame::unpack(&v).unwrap();
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
        let v = vec![0x24, 0x00, 0x00, 0x14, 0xca, 0x41, 0x37, 0x05];
        let frame = Frame::unpack(&v).unwrap();
        println!("{:?}", frame);

        // 00010100 00000000

        assert_eq!(frame.size, 0x0024);
        assert_eq!(frame.origin, 0);
        assert_eq!(frame.tagged, false);
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
            sequence: 248,
        };
        frame.validate();

        let v = frame.pack().unwrap();
        assert_eq!(v.len(), FrameAddress::packed_size());
        println!("Packed FrameAddress: {:?}", v);

        let unpacked = FrameAddress::unpack(&v).unwrap();
        assert_eq!(frame, unpacked);
    }

    #[test]
    fn test_decode_frame_address() {
        //   1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16
        let v = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x9c,
        ];
        assert_eq!(v.len(), FrameAddress::packed_size());

        let frame = FrameAddress::unpack(&v).unwrap();
        frame.validate();
        println!("FrameAddress: {:?}", frame);
    }

    #[test]
    fn test_protocol_header() {
        let frame = ProtocolHeader {
            reserved: 0,
            reserved2: 0,
            typ: 0x4455,
        };
        frame.validate();

        let v = frame.pack().unwrap();
        assert_eq!(v.len(), ProtocolHeader::packed_size());
        println!("Packed ProtocolHeader: {:?}", v);

        let unpacked = ProtocolHeader::unpack(&v).unwrap();
        assert_eq!(frame, unpacked);
    }

    #[test]
    fn test_decode_protocol_header() {
        //   1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16
        let v = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00,
        ];
        assert_eq!(v.len(), ProtocolHeader::packed_size());

        let frame = ProtocolHeader::unpack(&v).unwrap();
        frame.validate();
        println!("ProtocolHeader: {:?}", frame);
    }

    #[test]
    fn test_decode_full() {
        let v = vec![
            0x24, 0x00, 0x00, 0x14, 0xca, 0x41, 0x37, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x98, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x33, 0x00, 0x00, 0x00,
        ];

        let msg = RawMessage::unpack(&v).unwrap();
        msg.validate();
        println!("{:#?}", msg);
    }

    #[test]
    fn test_decode_full_1() {
        let v = vec![
            0x58, 0x00, 0x00, 0x54, 0xca, 0x41, 0x37, 0x05, 0xd0, 0x73, 0xd5, 0x02, 0x97, 0xde,
            0x00, 0x00, 0x4c, 0x49, 0x46, 0x58, 0x56, 0x32, 0x00, 0xc0, 0x44, 0x30, 0xeb, 0x47,
            0xc4, 0x48, 0x18, 0x14, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff,
            0xb8, 0x0b, 0x00, 0x00, 0xff, 0xff, 0x4b, 0x69, 0x74, 0x63, 0x68, 0x65, 0x6e, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        let msg = RawMessage::unpack(&v).unwrap();
        msg.validate();
        println!("{:#?}", msg);
    }

    #[test]
    fn test_build_a_packet() {
        // packet taken from https://lan.developer.lifx.com/docs/building-a-lifx-packet

        let msg = Message::LightSetColor {
            reserved: 0,
            color: HSBK {
                hue: 21845,
                saturation: 0xffff,
                brightness: 0xffff,
                kelvin: 3500,
            },
            duration: 1024,
        };

        let raw = RawMessage::build(
            &BuildOptions {
                target: None,
                ack_required: false,
                res_required: false,
                sequence: 0,
                source: 0,
            },
            msg,
        )
        .unwrap();

        let bytes = raw.pack().unwrap();
        println!("{:?}", bytes);
        assert_eq!(bytes.len(), 49);
        assert_eq!(
            bytes,
            vec![
                0x31, 0x00, 0x00, 0x34, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x66, 0x00, 0x00, 0x00, 0x00, 0x55, 0x55, 0xFF, 0xFF, 0xFF,
                0xFF, 0xAC, 0x0D, 0x00, 0x04, 0x00, 0x00
            ]
        );
    }
}
