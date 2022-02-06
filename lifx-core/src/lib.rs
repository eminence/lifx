//! This crate provides low-level message types and structures for dealing with the LIFX LAN protocol.
//!
//! This lets you control lights on your local area network.  More info can be found here:
//! <https://lan.developer.lifx.com/>
//!
//! Since this is a low-level library, it does not deal with issues like talking to the network,
//! caching light state, or waiting for replies.  This should be done at a higher-level library.
//!
//! # Discovery
//!
//! To discover lights on your LAN, send a [Message::GetService] message as a UDP broadcast to port 56700.
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
use std::cmp::PartialEq;
use std::convert::{TryFrom, TryInto};
use std::ffi::{CStr, CString};
use std::io;
use std::io::Cursor;
use std::num::NonZeroU8;
use thiserror::Error;

#[cfg(fuzzing)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[derive(Debug, Clone)]
pub struct ComparableFloat(f32);
#[cfg(fuzzing)]
impl PartialEq for ComparableFloat {
    fn eq(&self, other: &Self) -> bool {
        if self.0.is_nan() && other.0.is_nan() {
            true
        } else {
            self.0 == other.0
        }
    }
}
#[cfg(fuzzing)]
impl From<f32> for ComparableFloat {
    fn from(f: f32) -> Self {
        ComparableFloat(f)
    }
}

/// Various message encoding/decoding errors
#[derive(Error, Debug)]
pub enum Error {
    /// This error means we were unable to parse a raw message because its type is unknown.
    ///
    /// LIFX devices are known to send messages that are not officially documented, so this error
    /// type does not necessarily represent a bug.
    #[error("unknown message type: `{0}`")]
    UnknownMessageType(u16),
    /// This error means one of the message fields contains an invalid or unsupported value.
    #[error("protocol error: `{0}`")]
    ProtocolError(String),

    #[error("i/o error")]
    Io(#[from] io::Error),
}

impl From<std::convert::Infallible> for Error {
    fn from(_: std::convert::Infallible) -> Self {
        unreachable!()
    }
}

impl TryFrom<u8> for ApplicationRequest {
    type Error = Error;
    fn try_from(val: u8) -> Result<ApplicationRequest, Error> {
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

impl TryFrom<u8> for Waveform {
    type Error = Error;
    fn try_from(val: u8) -> Result<Waveform, Error> {
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

impl TryFrom<u8> for Service {
    type Error = Error;
    fn try_from(val: u8) -> Result<Service, Error> {
        match val {
            x if x == Service::UDP as u8 => Ok(Service::UDP),
            x if x == Service::Reserved1 as u8 => Ok(Service::Reserved1),
            x if x == Service::Reserved2 as u8 => Ok(Service::Reserved2),
            x if x == Service::Reserved3 as u8 => Ok(Service::Reserved3),
            x if x == Service::Reserved4 as u8 => Ok(Service::Reserved4),
            val => Err(Error::ProtocolError(format!(
                "Unknown service value {}",
                val
            ))),
        }
    }
}

impl TryFrom<u16> for PowerLevel {
    type Error = Error;
    fn try_from(val: u16) -> Result<PowerLevel, Error> {
        match val {
            x if x == PowerLevel::Enabled as u16 => Ok(PowerLevel::Enabled),
            x if x == PowerLevel::Standby as u16 => Ok(PowerLevel::Standby),
            x => Err(Error::ProtocolError(format!("Unknown power level {}", x))),
        }
    }
}

#[derive(Copy, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(fuzzing, derive(PartialEq))]
pub struct EchoPayload(pub [u8; 64]);

impl std::fmt::Debug for EchoPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "<EchoPayload>")
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct LifxIdent(pub [u8; 16]);

/// Lifx strings are fixed-length (32-bytes maximum)
#[derive(Debug, Clone, PartialEq)]
pub struct LifxString(CString);

impl LifxString {
    /// Constructs a new LifxString, truncating to 32 characters and ensuring there's a null terminator
    pub fn new(s: &CStr) -> LifxString {
        let mut b = s.to_bytes().to_vec();
        if b.len() > 31 {
            b[31] = 0;
            let b = b[..32].to_vec();
            LifxString(unsafe {
                // Saftey: we created the null terminator above, and the rest of the bytes originally came from a CStr
                CString::from_vec_with_nul_unchecked(b)
            })
        } else {
            LifxString(s.to_owned())
        }
    }
    pub fn cstr(&self) -> &CStr {
        &self.0
    }
}

impl std::fmt::Display for LifxString {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(fmt, "{}", self.0.to_string_lossy())
    }
}

impl std::cmp::PartialEq<str> for LifxString {
    fn eq(&self, other: &str) -> bool {
        self.0.to_string_lossy() == other
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for LifxString {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        // first pick a random length, between 0 and 32
        let len: usize = u.int_in_range(0..=31)?;

        let mut v = Vec::new();
        for _ in 0..len {
            let b: NonZeroU8 = u.arbitrary()?;
            v.push(b);
        }

        let s = CString::from(v);
        assert!(s.to_bytes_with_nul().len() <= 32);
        Ok(LifxString(s))
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

#[cfg(fuzzing)]
impl<T: WriteBytesExt> LittleEndianWriter<ComparableFloat> for T {
    fn write_val(&mut self, v: ComparableFloat) -> Result<(), io::Error> {
        self.write_f32::<LittleEndian>(v.0)
    }
}

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
        let b = v.0.to_bytes();
        for idx in 0..32 {
            if idx >= b.len() {
                self.write_u8(0)?;
            } else {
                self.write_u8(b[idx])?;
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

impl<T> LittleEndianWriter<LastHevCycleResult> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: LastHevCycleResult) -> Result<(), io::Error> {
        self.write_u8(v as u8)
    }
}

impl<T> LittleEndianWriter<MultiZoneEffectType> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: MultiZoneEffectType) -> Result<(), io::Error> {
        self.write_u8(v as u8)
    }
}

impl<T> LittleEndianWriter<&[HSBK; 82]> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: &[HSBK; 82]) -> Result<(), io::Error> {
        for elem in v {
            self.write_val(*elem)?;
        }
        Ok(())
    }
}

impl<T> LittleEndianWriter<&[u8; 32]> for T
where
    T: WriteBytesExt,
{
    fn write_val(&mut self, v: &[u8; 32]) -> Result<(), io::Error> {
        self.write_all(v)
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

impl<R: ReadBytesExt> LittleEndianReader<bool> for R {
    fn read_val(&mut self) -> Result<bool, io::Error> {
        Ok(self.read_u8()? > 0)
    }
}

impl<R: ReadBytesExt> LittleEndianReader<LastHevCycleResult> for R {
    fn read_val(&mut self) -> Result<LastHevCycleResult, io::Error> {
        let val: u8 = self.read_val()?;
        match val {
            0 => Ok(LastHevCycleResult::Success),
            1 => Ok(LastHevCycleResult::Busy),
            2 => Ok(LastHevCycleResult::InterruptedByReset),
            3 => Ok(LastHevCycleResult::InterruptedByHomekit),
            4 => Ok(LastHevCycleResult::InterruptedByLan),
            5 => Ok(LastHevCycleResult::InterruptedByCloud),
            _ => Ok(LastHevCycleResult::None),
        }
    }
}

impl<R: ReadBytesExt> LittleEndianReader<MultiZoneEffectType> for R {
    fn read_val(&mut self) -> Result<MultiZoneEffectType, io::Error> {
        let val: u8 = self.read_val()?;
        match val {
            0 => Ok(MultiZoneEffectType::Off),
            1 => Ok(MultiZoneEffectType::Move),
            2 => Ok(MultiZoneEffectType::Reserved1),
            _ => Ok(MultiZoneEffectType::Reserved2),
        }
    }
}

impl<R: ReadBytesExt> LittleEndianReader<[u8; 32]> for R {
    fn read_val(&mut self) -> Result<[u8; 32], io::Error> {
        let mut data = [0; 32];
        self.read_exact(&mut data)?;
        Ok(data)
    }
}

impl<R: ReadBytesExt> LittleEndianReader<[HSBK; 82]> for R {
    fn read_val(&mut self) -> Result<[HSBK; 82], io::Error> {
        let mut data = [HSBK {
            hue: 0,
            saturation: 0,
            brightness: 0,
            kelvin: 0,
        }; 82];
        for x in &mut data {
            *x = self.read_val()?;
        }

        Ok(data)
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
        let mut bytes = Vec::new();
        for _ in 0..31 {
            let c: u8 = self.read_val()?;
            if let Some(b) = std::num::NonZeroU8::new(c) {
                bytes.push(b);
            }
        }
        // read the null terminator
        self.read_u8()?;

        Ok(LifxString(CString::from(bytes)))
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

impl<R: ReadBytesExt> LittleEndianReader<PowerLevel> for R {
    fn read_val(&mut self) -> Result<PowerLevel, io::Error> {
        let val: u16 = self.read_val()?;
        if val == 0 {
            Ok(PowerLevel::Standby)
        } else {
            Ok(PowerLevel::Enabled)
        }
    }
}

impl<R: ReadBytesExt> LittleEndianReader<Waveform> for R {
    fn read_val(&mut self) -> Result<Waveform, io::Error> {
        let v = self.read_u8()?;
        match v {
            0 => Ok(Waveform::Saw),
            1 => Ok(Waveform::Sine),
            2 => Ok(Waveform::HalfSign),
            3 => Ok(Waveform::Triangle),
            4 => Ok(Waveform::Pulse),
            _ => Ok(Waveform::Saw), // default
        }
    }
}

macro_rules! unpack {
    ($msg:ident, $typ:ident, $( $n:ident: $t:ty ),*) => {
        {
        let mut c = Cursor::new(&$msg.payload);
        $(
            let $n: $t = c.read_val()?;
        )*

            Message::$typ {
            $(
                    $n: $n.try_into()?,
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
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum Service {
    UDP = 1,
    Reserved1 = 2,
    Reserved2 = 3,
    Reserved3 = 4,
    Reserved4 = 5,
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum PowerLevel {
    Standby = 0,
    Enabled = 65535,
}

/// Controls how/when multizone devices apply color changes
///
/// See also [Message::SetColorZones].
#[repr(u8)]
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(fuzzing, derive(PartialEq))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(fuzzing, derive(PartialEq))]
pub enum Waveform {
    Saw = 0,
    Sine = 1,
    HalfSign = 2,
    Triangle = 3,
    Pulse = 4,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(fuzzing, derive(PartialEq))]
pub enum LastHevCycleResult {
    Success = 0,
    Busy = 1,
    InterruptedByReset = 2,
    InterruptedByHomekit = 3,
    InterruptedByLan = 4,
    InterruptedByCloud = 5,
    None = 255,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(fuzzing, derive(PartialEq))]
pub enum MultiZoneEffectType {
    Off = 0,
    Move = 1,
    Reserved1 = 2,
    Reserved2 = 3,
}

/// Decoded LIFX Messages
///
/// This enum lists all of the LIFX message types known to this library.
///
/// Note that other message types exist, but are not officially documented (and so are not
/// available here).
#[derive(Clone, Debug)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(fuzzing, derive(PartialEq))]
pub enum Message {
    /// Sent by a client to acquire responses from all devices on the local network. No payload is
    /// required. Causes the devices to transmit a [Message::StateService] message.
    ///
    /// Message type 2
    GetService,

    /// Response to [Message::GetService] message.
    ///
    /// You'll want to save the port number in this message, so you can send future messages directly
    /// to this device.
    ///
    /// Message type 3
    StateService {
        /// unsigned 8-bit integer, maps to `Service`
        service: Service,
        /// Port number of the light.  If the service is temporarily unavailable, then the port value
        /// will be 0.
        port: u32,
    },

    /// Get Host MCU information. No payload is required. Causes the device to transmit a
    /// [Message::StateHostInfo] message.
    ///
    /// Message type 12
    GetHostInfo,

    /// Response to [Message::GetHostInfo] message.
    ///
    /// Provides host MCU information.
    ///
    /// Message type 13
    StateHostInfo {
        /// radio receive signal strength in miliWatts
        #[cfg(not(fuzzing))]
        signal: f32,
        #[cfg(fuzzing)]
        signal: ComparableFloat,
        /// Bytes transmitted since power on
        tx: u32,
        /// Bytes received since power on
        rx: u32,
        reserved: i16,
    },

    /// Gets Host MCU firmware information
    ///
    /// Causes the device to transmit a [Message::StateHostFirmware] message.
    ///
    /// Message type 14
    GetHostFirmware,

    /// Response to [Message::GetHostFirmware] message.
    ///
    /// Provides host firmware information.
    ///
    /// Message type 15
    StateHostFirmware {
        /// Firmware build time (absolute time in nanoseconds since epoch)
        build: u64,
        reserved: u64,
        /// The minor component of the firmware version
        version_minor: u16,
        /// The major component of the firmware version
        version_major: u16,
    },

    /// Get Wifi subsystem information. No payload is required. Causes the device to transmit a
    /// [Message::StateWifiInfo] message.
    ///
    /// Message type 16
    GetWifiInfo,

    /// StateWifiInfo - 17
    ///
    /// Response to [Message::GetWifiInfo] message.
    ///
    /// Provides Wifi subsystem information.
    ///
    /// Message type 17
    StateWifiInfo {
        /// Radio receive signal strength
        ///
        /// The units of this field varies between different products.  See this LIFX doc for more info:
        /// <https://lan.developer.lifx.com/docs/information-messages#statewifiinfo---packet-17>
        #[cfg(not(fuzzing))]
        signal: f32,
        #[cfg(fuzzing)]
        signal: ComparableFloat,
        /// Reserved
        ///
        /// This field used to store bytes transmitted since power on
        reserved6: u32,
        /// Reserved
        ///
        /// This field used to store bytes received since power on
        reserved7: u32,
        reserved: i16,
    },

    /// Get Wifi subsystem firmware
    ///
    /// Causes the device to transmit a [Message::StateWifiFirmware] message.
    ///
    /// Message type 18
    GetWifiFirmware,

    /// Response to [Message::GetWifiFirmware] message.
    ///
    /// Provides Wifi subsystem information.
    ///
    /// Message type 19
    StateWifiFirmware {
        /// firmware build time (absolute time in nanoseconds since epoch)
        build: u64,
        reserved: u64,
        /// The minor component of the firmware version
        version_minor: u16,
        /// The major component of the firmware version
        version_major: u16,
    },

    /// Get device power level
    ///
    /// Causes the device to transmit a [Message::StatePower] message
    ///
    /// Message type 20
    GetPower,

    /// Set device power level.
    ///
    /// Message type 21
    SetPower {
        /// normally a u16, but only 0 and 65535 are supported.
        ///
        /// Zero implies standby and non-zero sets a corresponding power draw level.
        level: PowerLevel,
    },

    /// Response to [Message::GetPower] message.
    ///
    /// Provides device power level.
    ///
    /// Message type 22
    StatePower {
        /// The current level of the device's power
        ///
        /// A value of `0` means off, and any other value means on.  Note that `65535`
        /// is full power and during a power transition the value may be any value
        /// between `0` and `65535`.
        level: u16,
    },

    ///
    /// Get device label
    ///
    /// Causes the device to transmit a [Message::StateLabel] message.
    ///
    /// Message type 23
    GetLabel,

    /// Set the device label text.
    ///
    /// Message type 24
    SetLabel { label: LifxString },

    /// Response to [Message::GetLabel] message.
    ///
    /// Provides device label.
    ///
    /// Message type 25
    StateLabel { label: LifxString },

    /// Get the hardware version
    ///
    /// Causes the device to transmit a [Message::StateVersion] message.
    ///
    /// Message type 32
    GetVersion,

    /// Response to [Message::GetVersion] message.
    ///
    /// Provides the hardware version of the device. To get more information about this product,
    /// use the [get_product_info] function.
    ///
    /// Message type 33
    StateVersion {
        /// vendor ID
        ///
        /// For LIFX products, this value is `1`.
        vendor: u32,
        /// product ID
        product: u32,
        /// Reserved
        ///
        /// Previously, this field stored the hardware version
        reserved: u32,
    },

    /// Get run-time information
    ///
    /// Causes the device to transmit a [Message::StateInfo] message.
    ///
    /// Message type 34
    GetInfo,

    /// Response to [Message::GetInfo] message.
    ///
    /// Provides run-time information of device.
    ///
    /// Message type 35
    StateInfo {
        /// The current time according to the device
        ///
        /// Note that this is most likely inaccurate.
        ///
        /// (absolute time in nanoseconds since epoch)
        time: u64,
        /// The amount of time in nanoseconds the device has been online since last power on
        uptime: u64,
        /// The amount of time in nanseconds of power off time accurate to 5 seconds.
        downtime: u64,
    },

    /// Response to any message sent with ack_required set to 1. See message header frame address.
    ///
    /// (Note that technically this message has no payload, but the frame sequence number is stored
    /// here for convenience).
    ///
    /// Message type 45
    Acknowledgement { seq: u8 },

    /// Ask the bulb to return its location information
    ///
    /// Causes the device to transmit a [Message::StateLocation] message.
    ///
    /// Message type 48
    GetLocation,

    /// Set the device location
    ///
    /// Message type 49
    SetLocation {
        /// GUID byte array
        location: LifxIdent,
        /// The name assigned to this location
        label: LifxString,
        /// An epoch in nanoseconds of when this location was set on the device
        updated_at: u64,
    },

    /// Device location.
    ///
    /// Message type 50
    StateLocation {
        location: LifxIdent,
        label: LifxString,
        updated_at: u64,
    },

    /// Ask the bulb to return its group membership information
    ///
    /// Causes the device to transmit a [Message::StateGroup] message.
    ///
    /// Message type 51
    GetGroup,

    /// Set the device group
    ///
    /// Message type 52
    SetGroup {
        group: LifxIdent,
        label: LifxString,
        updated_at: u64,
    },

    /// Device group.
    ///
    /// Message type 53
    StateGroup {
        /// The unique identifier of this group as a `uuid`.
        group: LifxIdent,
        /// The name assigned to this group
        label: LifxString,
        /// An epoch in nanoseconds of when this group was set on the device
        updated_at: u64,
    },

    /// Request an arbitrary payload be echoed back
    ///
    /// Causes the device to transmit an [Message::EchoResponse] message.
    ///
    /// Message type 58
    EchoRequest { payload: EchoPayload },

    /// Response to [Message::EchoRequest] message.
    ///
    /// Echo response with payload sent in the EchoRequest.
    ///
    /// Message type 59
    EchoResponse { payload: EchoPayload },

    /// Sent by a client to obtain the light state.
    ///
    /// Causes the device to transmit a [Message::LightState] message.
    ///
    /// Note: this message is also known as `GetColor` in the LIFX docs.  Message type 101
    LightGet,

    /// Sent by a client to change the light state.
    ///
    /// If the Frame Address res_required field is set to one (1) then the device will transmit a
    /// State message.
    ///
    /// Message type 102
    LightSetColor {
        reserved: u8,
        /// Color in HSBK
        color: HSBK,
        /// Color transition time in milliseconds
        duration: u32,
    },

    /// Apply an effect to the bulb.
    ///
    /// Message type 103
    SetWaveform {
        reserved: u8,
        transient: bool,
        color: HSBK,
        /// Duration of a cycle in milliseconds
        period: u32,
        /// Number of cycles
        #[cfg(not(fuzzing))]
        cycles: f32,
        #[cfg(fuzzing)]
        cycles: ComparableFloat,
        /// Waveform Skew, [-32768, 32767] scaled to [0, 1].
        skew_ratio: i16,
        /// Waveform to use for transition.
        waveform: Waveform,
    },

    /// Sent by a device to provide the current light state.
    ///
    /// This message is sent in reply to [Message::LightGet], [Message::LightSetColor], [Message::SetWaveform], and [Message::SetWaveformOptional]
    ///
    /// Message type 107
    LightState {
        color: HSBK,
        reserved: i16,
        /// The current power level of the device
        power: u16,
        /// The current label on the device
        label: LifxString,
        reserved2: u64,
    },

    /// Sent by a client to obtain the power level
    ///
    /// Causes the device to transmit a [Message::LightStatePower] message.
    ///
    /// Message type 116
    LightGetPower,

    /// Sent by a client to change the light power level.
    ///
    /// The duration is the power level transition time in milliseconds.
    ///
    /// If the Frame Address res_required field is set to one (1) then the device will transmit a
    /// StatePower message.
    ///
    /// Message type 117
    LightSetPower { level: u16, duration: u32 },

    /// Sent by a device to provide the current power level.
    ///
    /// Message type 118
    LightStatePower { level: u16 },

    /// Apply an effect to the bulb.
    ///
    /// Message type 119
    SetWaveformOptional {
        reserved: u8,
        transient: bool,
        color: HSBK,
        /// Duration of a cycle in milliseconds
        period: u32,
        /// Number of cycles
        #[cfg(not(fuzzing))]
        cycles: f32,
        #[cfg(fuzzing)]
        cycles: ComparableFloat,

        skew_ratio: i16,
        waveform: Waveform,
        set_hue: bool,
        set_saturation: bool,
        set_brightness: bool,
        set_kelvin: bool,
    },

    /// Gets the current maximum power level of the Infraed channel
    ///
    /// Message type 120
    LightGetInfrared,

    /// Indicates the current maximum setting for the infrared channel.
    ///
    /// Message type 121
    LightStateInfrared { brightness: u16 },

    /// Set the current maximum brightness for the infrared channel.
    ///
    /// Message type 122
    LightSetInfrared { brightness: u16 },

    /// Get the state of the HEV LEDs on the device
    ///
    /// Causes the device to transmite a [Messages::LightStateHevCycle] message.
    ///
    /// This message requires the device has the `hev` capability
    ///
    /// Message type 142
    LightGetHevCycle,

    /// Message type 143
    LightSetHevCycle {
        /// Set this to false to turn off the cycle and true to start the cycle
        enable: bool,
        /// The duration, in seconds that the cycle should last for
        ///
        /// A value of 0 will use the default duration set by SetHevCycleConfiguration (146).
        duration: u32,
    },

    /// Whether a HEV cycle is running on the device
    ///
    /// Message type 144
    LightStateHevCycle {
        /// The duration, in seconds, this cycle was set to
        duration: u32,
        /// The duration, in seconds, remaining in this cycle
        remaining: u32,
        /// The power state before the HEV cycle started, which will be the power state once the cycle completes.
        ///
        /// This is only relevant if `remaining` is larger than 0.
        last_power: bool,
    },

    /// Getthe default configuration for using the HEV LEDs on the device
    ///
    /// This message requires the device has the `hev` capability
    ///
    /// Message type 145
    LightGetHevCycleConfiguration,

    /// Message type 146
    LightSetHevCycleConfiguration { indication: bool, duration: u32 },

    /// Message type 147
    LightStateHevCycleConfiguration { indication: bool, duration: u32 },

    /// Message type 148
    LightGetLastHevCycleResult,

    /// Message type 149
    LightStateLastHevCycleResult { result: LastHevCycleResult },

    /// This message is used for changing the color of either a single or multiple zones.
    /// The changes are stored in a buffer and are only applied once a message with either
    /// [ApplicationRequest::Apply] or [ApplicationRequest::ApplyOnly] set.
    ///
    /// Message type 501
    SetColorZones {
        start_index: u8,
        end_index: u8,
        color: HSBK,
        duration: u32,
        apply: ApplicationRequest,
    },

    /// GetColorZones is used to request the zone colors for a range of zones.
    ///
    /// The bulb will respond
    /// with either [Message::StateZone] or [Message::StateMultiZone] messages as required to cover
    /// the requested range. The bulb may send state messages that cover more than the requested
    /// zones. Any zones outside the requested indexes will still contain valid values at the time
    /// the message was sent.
    ///
    /// Message type 502
    GetColorZones { start_index: u8, end_index: u8 },

    /// The StateZone message represents the state of a single zone with the `index` field indicating
    /// which zone is represented. The `count` field contains the count of the total number of zones
    /// available on the device.
    ///
    /// Message type 503
    StateZone { count: u8, index: u8, color: HSBK },

    /// The StateMultiZone message represents the state of eight consecutive zones in a single message.
    /// As in the StateZone message the `count` field represents the count of the total number of
    /// zones available on the device. In this message the `index` field represents the index of
    /// `color0` and the rest of the colors are the consecutive zones thus the index of the
    /// `color_n` zone will be `index + n`.
    ///
    /// Message type 506
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

    /// Message type 507
    GetMultiZoneEffect,

    /// Message type 509
    StateMultiZoneEffect {
        /// The unique value identifying this effect
        instance_id: u32,
        typ: MultiZoneEffectType,
        reserved: u16,
        /// The time it takes for one cycle of the effect in milliseconds
        speed: u32,
        /// The amount of time left in the current effect in nanoseconds
        duration: u64,
        reserved7: u32,
        reserved8: u32,
        /// The parameters that was used in the request.
        parameters: [u8; 32],
    },

    /// Message type 511
    GetExtendedColorZone,

    /// Message type 512
    StateExtendedColorZones {
        zones_count: u16,
        zone_index: u16,
        colors_count: u8,
        colors: [HSBK; 82],
    },

    /// Get the power state of a relay
    ///
    /// This requires the device has the `relays` capability.
    ///
    /// Message type 816
    RelayGetPower {
        /// The relay on the switch starting from 0
        relay_index: u8,
    },

    /// Message ty 817
    RelaySetPower {
        /// The relay on the switch starting from 0
        relay_index: u8,
        /// The value of the relay
        ///
        /// Current models of the LIFX switch do not have dimming capability, so the two valid values are `0`
        /// for off and `65535` for on.
        level: u16,
    },

    /// The state of the device relay
    ///
    /// Message type 818
    RelayStatePower {
        /// The relay on the switch starting from 0
        relay_index: u8,
        /// The value of the relay
        ///
        /// Current models of the LIFX switch do not have dimming capability, so the two valid values are `0`
        /// for off and `65535` for on.
        level: u16,
    },
}

impl Message {
    /// Get the message type
    ///
    /// This will be used in the `typ` field of the [ProtocolHeader].
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
            Message::LightGetHevCycle => 142,
            Message::LightSetHevCycle { .. } => 143,
            Message::LightStateHevCycle { .. } => 144,
            Message::LightGetHevCycleConfiguration => 145,
            Message::LightSetHevCycleConfiguration { .. } => 146,
            Message::LightStateHevCycleConfiguration { .. } => 147,
            Message::LightGetLastHevCycleResult => 148,
            Message::LightStateLastHevCycleResult { .. } => 149,
            Message::SetColorZones { .. } => 501,
            Message::GetColorZones { .. } => 502,
            Message::StateZone { .. } => 503,
            Message::StateMultiZone { .. } => 506,
            Message::GetMultiZoneEffect => 507,
            Message::StateMultiZoneEffect { .. } => 509,
            Message::GetExtendedColorZone => 511,
            Message::StateExtendedColorZones { .. } => 512,
            Message::RelayGetPower { .. } => 816,
            Message::RelaySetPower { .. } => 817,
            Message::RelayStatePower { .. } => 818,
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
                version_minor: u16,
                version_major: u16
            )),
            16 => Ok(Message::GetWifiInfo),
            17 => Ok(unpack!(
                msg,
                StateWifiInfo,
                signal: f32,
                reserved6: u32,
                reserved7: u32,
                reserved: i16
            )),
            18 => Ok(Message::GetWifiFirmware),
            19 => Ok(unpack!(
                msg,
                StateWifiFirmware,
                build: u64,
                reserved: u64,
                version_minor: u16,
                version_major: u16
            )),
            20 => Ok(Message::GetPower),
            21 => Ok(unpack!(msg, SetPower, level: PowerLevel)),
            22 => Ok(unpack!(msg, StatePower, level: u16)),
            23 => Ok(Message::GetLabel),
            24 => Ok(unpack!(msg, SetLabel, label: LifxString)),
            25 => Ok(unpack!(msg, StateLabel, label: LifxString)),
            32 => Ok(Message::GetVersion),
            33 => Ok(unpack!(
                msg,
                StateVersion,
                vendor: u32,
                product: u32,
                reserved: u32
            )),
            34 => Ok(Message::GetInfo),
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
            49 => Ok(unpack!(
                msg,
                SetLocation,
                location: LifxIdent,
                label: LifxString,
                updated_at: u64
            )),
            50 => Ok(unpack!(
                msg,
                StateLocation,
                location: LifxIdent,
                label: LifxString,
                updated_at: u64
            )),
            51 => Ok(Message::GetGroup),
            52 => Ok(unpack!(
                msg,
                SetGroup,
                group: LifxIdent,
                label: LifxString,
                updated_at: u64
            )),
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
            103 => Ok(unpack!(
                msg,
                SetWaveform,
                reserved: u8,
                transient: bool,
                color: HSBK,
                period: u32,
                cycles: f32,
                skew_ratio: i16,
                waveform: Waveform
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
            119 => Ok(unpack!(
                msg,
                SetWaveformOptional,
                reserved: u8,
                transient: bool,
                color: HSBK,
                period: u32,
                cycles: f32,
                skew_ratio: i16,
                waveform: Waveform,
                set_hue: bool,
                set_saturation: bool,
                set_brightness: bool,
                set_kelvin: bool
            )),
            120 => Ok(Message::LightGetInfrared),
            122 => Ok(unpack!(msg, LightSetInfrared, brightness: u16)),
            142 => Ok(Message::LightGetHevCycle),
            143 => Ok(unpack!(msg, LightSetHevCycle, enable: bool, duration: u32)),
            144 => Ok(unpack!(
                msg,
                LightStateHevCycle,
                duration: u32,
                remaining: u32,
                last_power: bool
            )),
            145 => Ok(Message::LightGetHevCycleConfiguration),
            146 => Ok(unpack!(
                msg,
                LightSetHevCycleConfiguration,
                indication: bool,
                duration: u32
            )),
            147 => Ok(unpack!(
                msg,
                LightStateHevCycleConfiguration,
                indication: bool,
                duration: u32
            )),
            148 => Ok(Message::LightGetLastHevCycleResult),
            149 => Ok(unpack!(
                msg,
                LightStateLastHevCycleResult,
                result: LastHevCycleResult
            )),
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
            507 => Ok(Message::GetMultiZoneEffect),
            509 => Ok(unpack!(
                msg,
                StateMultiZoneEffect,
                instance_id: u32,
                typ: MultiZoneEffectType,
                reserved: u16,
                speed: u32,
                duration: u64,
                reserved7: u32,
                reserved8: u32,
                parameters: [u8; 32]
            )),
            511 => Ok(Message::GetExtendedColorZone),
            512 => Ok(unpack!(
                msg,
                StateExtendedColorZones,
                zones_count: u16,
                zone_index: u16,
                colors_count: u8,
                colors: [HSBK; 82]
            )),
            816 => Ok(unpack!(msg, RelayGetPower, relay_index: u8)),
            817 => Ok(unpack!(msg, RelaySetPower, relay_index: u8, level: u16)),
            818 => Ok(unpack!(msg, RelayStatePower, relay_index: u8, level: u16)),
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
    ///
    /// See also [Message::get_num]
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
            | Message::LightGetInfrared
            | Message::LightGetHevCycle
            | Message::LightGetHevCycleConfiguration
            | Message::LightGetLastHevCycleResult
            | Message::GetMultiZoneEffect
            | Message::GetExtendedColorZone => {
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
                v.write_val(service as u8)?;
                v.write_val(port)?;
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
                version_minor,
                version_major,
            } => {
                v.write_val(build)?;
                v.write_val(reserved)?;
                v.write_val(version_minor)?;
                v.write_val(version_major)?;
            }
            Message::StateWifiInfo {
                signal,
                reserved6,
                reserved7,
                reserved,
            } => {
                v.write_val(signal)?;
                v.write_val(reserved6)?;
                v.write_val(reserved7)?;
                v.write_val(reserved)?;
            }
            Message::StateWifiFirmware {
                build,
                reserved,
                version_minor,
                version_major,
            } => {
                v.write_val(build)?;
                v.write_val(reserved)?;
                v.write_val(version_minor)?;
                v.write_val(version_major)?;
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
                reserved,
            } => {
                v.write_val(vendor)?;
                v.write_val(product)?;
                v.write_val(reserved)?;
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
            Message::LightStateHevCycle {
                duration,
                remaining,
                last_power,
            } => {
                v.write_val(duration)?;
                v.write_val(remaining)?;
                v.write_val(last_power)?;
            }
            Message::LightStateHevCycleConfiguration {
                indication,
                duration,
            } => {
                v.write_val(indication)?;
                v.write_val(duration)?;
            }
            Message::LightStateLastHevCycleResult { result } => {
                v.write_val(result)?;
            }
            Message::StateMultiZoneEffect {
                instance_id,
                typ,
                reserved,
                speed,
                duration,
                reserved7,
                reserved8,
                parameters,
            } => {
                v.write_val(instance_id)?;
                v.write_val(typ)?;
                v.write_val(reserved)?;
                v.write_val(speed)?;
                v.write_val(duration)?;
                v.write_val(reserved7)?;
                v.write_val(reserved8)?;
                v.write_val(&parameters)?;
            }
            Message::StateExtendedColorZones {
                zones_count,
                zone_index,
                colors_count,
                colors,
            } => {
                v.write_val(zones_count)?;
                v.write_val(zone_index)?;
                v.write_val(colors_count)?;
                v.write_val(&colors)?;
            }
            Message::RelayGetPower { relay_index } => {
                v.write_val(relay_index)?;
            }
            Message::RelayStatePower { relay_index, level } => {
                v.write_val(relay_index)?;
                v.write_val(level)?;
            }
            Message::RelaySetPower { relay_index, level } => {
                v.write_val(relay_index)?;
                v.write_val(level)?;
            }
            Message::LightSetHevCycle { enable, duration } => {
                v.write_val(enable)?;
                v.write_val(duration)?;
            }
            Message::LightSetHevCycleConfiguration {
                indication,
                duration,
            } => {
                v.write_val(indication)?;
                v.write_val(duration)?;
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

#[derive(Debug, Clone)]
pub enum TemperatureRange {
    /// The device supports a range of temperatures
    Variable { min: u16, max: u16 },
    /// The device only supports 1 temperature
    Fixed(u16),
    /// For devices that aren't lighting products (the LIFX switch)
    None,
}

#[derive(Clone, Debug)]
pub struct ProductInfo {
    pub name: &'static str,

    /// The light changes physical appearance when the Hue value is changed
    pub color: bool,

    /// The light supports emitting infrared light
    pub infrared: bool,

    /// The light supports a 1D linear array of LEDs (the Z and Beam)
    pub multizone: bool,

    /// The light may be connected to physically separated hardware (currently only the LIFX Tile)
    pub chain: bool,

    /// The light supports emitted HEV light
    pub hev: bool,

    /// The light supports a 2D matrix of LEDs (the Tile and Candle)
    pub matrix: bool,

    /// The device has relays for controlling physical power to something (the LIFX switch)
    pub relays: bool,

    /// The device has physical buttons to press (the LIFX switch)
    pub buttons: bool,

    /// The temperature range this device supports
    pub temperature_range: TemperatureRange,
}

/// Look up info about what a LIFX product supports.
///
/// You can get the vendor and product IDs from a bulb by receiving a [Message::StateVersion] message
///
/// Data is taken from <https://github.com/LIFX/products/blob/master/products.json>
#[rustfmt::skip]
pub fn get_product_info(vendor: u32, product: u32) -> Option<&'static ProductInfo> {
    match (vendor, product) {
        (1, 1) => Some(&ProductInfo { name: "LIFX Original 1000", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, 
        temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 3) => Some(&ProductInfo { name: "LIFX Color 650", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 10) => Some(&ProductInfo { name: "LIFX White 800 (Low Voltage)", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 6500 }  }),
        (1, 11) => Some(&ProductInfo { name: "LIFX White 800 (High Voltage)", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 6500 }  }),
        (1, 15) => Some(&ProductInfo { name: "LIFX Color 1000", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 18) => Some(&ProductInfo { name: "LIFX White 900 BR30 (Low Voltage)", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 19) => Some(&ProductInfo { name: "LIFX White 900 BR30 (High Voltage)", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 20) => Some(&ProductInfo { name: "LIFX Color 1000 BR30", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 22) => Some(&ProductInfo { name: "LIFX Color 1000", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 27) => Some(&ProductInfo { name: "LIFX A19", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 28) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 29) => Some(&ProductInfo { name: "LIFX A19 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 30) => Some(&ProductInfo { name: "LIFX BR30 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 31) => Some(&ProductInfo { name: "LIFX Z", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 32) => Some(&ProductInfo { name: "LIFX Z", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 36) => Some(&ProductInfo { name: "LIFX Downlight", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 37) => Some(&ProductInfo { name: "LIFX Downlight", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 38) => Some(&ProductInfo { name: "LIFX Beam", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 39) => Some(&ProductInfo { name: "LIFX Downlight White to Warm", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 40) => Some(&ProductInfo { name: "LIFX Downlight", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 43) => Some(&ProductInfo { name: "LIFX A19", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 44) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 45) => Some(&ProductInfo { name: "LIFX A19 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 46) => Some(&ProductInfo { name: "LIFX BR30 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 49) => Some(&ProductInfo { name: "LIFX Mini Color", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 50) => Some(&ProductInfo { name: "LIFX Mini White to Warm", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: 
        false, temperature_range: TemperatureRange::Variable { min: 1500, max: 6500 }  }),
        (1, 51) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 52) => Some(&ProductInfo { name: "LIFX GU10", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 53) => Some(&ProductInfo { name: "LIFX GU10", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 55) => Some(&ProductInfo { name: "LIFX Tile", color: true, infrared: false, multizone: false, chain: true, hev: false, matrix: true, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2500, max: 9000 }  }),
        (1, 57) => Some(&ProductInfo { name: "LIFX Candle", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: true, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 59) => Some(&ProductInfo { name: "LIFX Mini Color", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 60) => Some(&ProductInfo { name: "LIFX Mini White to Warm", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: 
        false, temperature_range: TemperatureRange::Variable { min: 1500, max: 6500 }  }),
        (1, 61) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 62) => Some(&ProductInfo { name: "LIFX A19", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 63) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 64) => Some(&ProductInfo { name: "LIFX A19 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 65) => Some(&ProductInfo { name: "LIFX BR30 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 66) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 68) => Some(&ProductInfo { name: "LIFX Candle", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: true, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 70) => Some(&ProductInfo { name: "LIFX Switch", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: true, buttons: true, temperature_range: TemperatureRange::None }),
        (1, 71) => Some(&ProductInfo { name: "LIFX Switch", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: true, buttons: true, temperature_range: TemperatureRange::None }),
        (1, 81) => Some(&ProductInfo { name: "LIFX Candle White to Warm", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2200, max: 6500 }  }),
        (1, 82) => Some(&ProductInfo { name: "LIFX Filament Clear", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2100, max: 2100 }  }),
        (1, 85) => Some(&ProductInfo { name: "LIFX Filament Amber", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2000, max: 2000 }  }),
        (1, 87) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 88) => Some(&ProductInfo { name: "LIFX Mini White", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 89) => Some(&ProductInfo { name: "LIFX Switch", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: true, buttons: true, temperature_range: TemperatureRange::None }),
        (1, 90) => Some(&ProductInfo { name: "LIFX Clean", color: true, infrared: false, multizone: false, chain: false, hev: true, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 91) => Some(&ProductInfo { name: "LIFX Color", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 92) => Some(&ProductInfo { name: "LIFX Color", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 93) => Some(&ProductInfo { name: "LIFX A19 US", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 94) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 96) => Some(&ProductInfo { name: "LIFX Candle White to Warm", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2200, max: 6500 }  }),
        (1, 97) => Some(&ProductInfo { name: "LIFX A19", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 98) => Some(&ProductInfo { name: "LIFX BR30", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 99) => Some(&ProductInfo { name: "LIFX Clean", color: true, infrared: false, multizone: false, chain: false, hev: true, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 100) => Some(&ProductInfo { name: "LIFX Filament Clear", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2100, max: 2100 }  }),
        (1, 101) => Some(&ProductInfo { name: "LIFX Filament Amber", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2000, max: 2000 }  }),
        (1, 109) => Some(&ProductInfo { name: "LIFX A19 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 110) => Some(&ProductInfo { name: "LIFX BR30 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 111) => Some(&ProductInfo { name: "LIFX A19 Night Vision", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 112) => Some(&ProductInfo { name: "LIFX BR30 Night Vision Intl", color: true, infrared: true, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 113) => Some(&ProductInfo { name: "LIFX Mini WW US", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, 
        temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 114) => Some(&ProductInfo { name: "LIFX Mini WW Intl", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 115) => Some(&ProductInfo { name: "LIFX Switch", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: true, buttons: true, temperature_range: TemperatureRange::None }),
        (1, 116) => Some(&ProductInfo { name: "LIFX Switch", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: true, buttons: true, temperature_range: TemperatureRange::None }),
        (1, 117) => Some(&ProductInfo { name: "LIFX Z", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 118) => Some(&ProductInfo { name: "LIFX Z", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 119) => Some(&ProductInfo { name: "LIFX Beam", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 120) => Some(&ProductInfo { name: "LIFX Beam", color: true, infrared: false, multizone: true, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 123) => Some(&ProductInfo { name: "LIFX Color US", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 124) => Some(&ProductInfo { name: "LIFX Color Intl", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 125) => Some(&ProductInfo { name: "LIFX White to Warm US", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 126) => Some(&ProductInfo { name: "LIFX White to Warm Intl", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 127) => Some(&ProductInfo { name: "LIFX White US", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 128) => Some(&ProductInfo { name: "LIFX White Intl", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, 
        temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 129) => Some(&ProductInfo { name: "LIFX Color US", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 130) => Some(&ProductInfo { name: "LIFX Color Intl", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 131) => Some(&ProductInfo { name: "LIFX White To Warm US", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 132) => Some(&ProductInfo { name: "LIFX White To Warm Intl", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 133) => Some(&ProductInfo { name: "LIFX White US", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 134) => Some(&ProductInfo { name: "LIFX White Intl", color: false, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, 
        temperature_range: TemperatureRange::Variable { min: 2700, max: 2700 }  }),
        (1, 135) => Some(&ProductInfo { name: "LIFX GU10 Color US", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 136) => Some(&ProductInfo { name: "LIFX GU10 Color Intl", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: false, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 137) => Some(&ProductInfo { name: "LIFX Candle Color US", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: true, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
        (1, 138) => Some(&ProductInfo { name: "LIFX Candle Color Intl", color: true, infrared: false, multizone: false, chain: false, hev: false, matrix: true, relays: false, buttons: false, temperature_range: TemperatureRange::Variable { min: 1500, max: 9000 }  }),
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

    #[test]
    fn test_lifx_string() {
        let s = CStr::from_bytes_with_nul(b"hello\0").unwrap();
        let ls = LifxString::new(s);
        assert_eq!(ls.cstr(), s);
        assert!(ls.cstr().to_bytes_with_nul().len() <= 32);

        let s = CStr::from_bytes_with_nul(b"this is bigger than thirty two characters\0").unwrap();
        let ls = LifxString::new(s);
        assert_eq!(ls.cstr().to_bytes_with_nul().len(), 32);
        assert_eq!(
            ls.cstr(),
            CStr::from_bytes_with_nul(b"this is bigger than thirty two \0").unwrap()
        );
    }
}
