#![feature(io_error_more)]

// #![feature(never_type)] // Not implemented within serde, thus remaking my own for the time being

// #![feature(try_trait_v2)]
// But i like ? very much :(
#![allow(clippy::try_err)]
#![allow(clippy::needless_question_mark)]

pub mod types;

use std::fmt::{Display, Formatter};
use types::*;

use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::io::{Result as IOResult, Error as IOError, ErrorKind as IOErrorKind, Write, Read};
use epoll_rs::{Epoll, Opts as PollOpts};
use serde::{ser::Serialize, de::{ Deserialize, DeserializeOwned, Visitor, SeqAccess, Error}};
use stack_buffer::{StackBufReader};
use arrayvec::ArrayVec;

/// Used as the delimiter for HLAPI JSON packets
pub const DELIM: &[u8] = b"\0";

/// There's no practical limit when sending from Java to OC2 VMs
const READ_BUF: usize = 4096; // TODO: Benchmark different sizes trough file importing

/// Maximum size for sending buffers, limitation from OC2 VMs to Java
const MAX_WRITE: usize = 4096; // TODO: try using buffers and benchmark

/// Main bus path
const MAIN_BUS: &str = "/dev/hvc0";

pub struct HLAPIBus {
    handle: File,
    poller: Epoll,
}

impl HLAPIBus {
    pub fn main_bus() -> IOResult<Self> {
        let poller = Epoll::new()?;
        let handle = poller.add(File::options().read(true).write(true).open(MAIN_BUS)?, PollOpts::IN)?.into_file();

        let descriptor = handle.as_raw_fd();
        let mut termios = termios::Termios::from_fd(descriptor)?;

        termios::cfmakeraw(&mut termios); // raw
        termios.c_lflag &= !termios::ECHO; // -echo
        termios::tcsetattr(descriptor, termios::TCSANOW, &termios)?; // immediate flush

        termios::cfsetspeed(&mut termios, termios::B38400)?; // baud 38400 // TODO: try faster BAUD rates

        Ok(Self { handle, poller })
    }

    pub fn list(&mut self) -> IOResult<Vec<HLAPIDeviceDescriptor>> {
        self.write::<&'static str, Empty>(&HLAPISend::List)?;
        let list: HLAPIReceive = self.read()?;
        if let HLAPIReceive::List(devices) = list {
            Ok(devices)
        } else { Err(IOErrorKind::InvalidData.into()) }
    }

    pub fn methods(&mut self, device: HLAPIDeviceHandle) -> IOResult<Vec<HLAPIMethod>> {
        self.write::<&'static str, Empty>(&HLAPISend::Methods(device))?;
        let list: HLAPIReceive = self.read()?;
        if let HLAPIReceive::Methods(methods) = list {
            Ok(methods)
        } else { Err(IOErrorKind::InvalidData.into()) }
    }

    pub fn find(&mut self, name: &str) -> IOResult<HLAPIDeviceHandle> {
        for HLAPIDeviceDescriptor { device_id, components } in self.list()? {
            if components.into_iter().any(|dev| name == dev) { return Ok(device_id); }
        }
        Err(IOErrorKind::NotFound.into())
    }

    pub fn raw_call<Name: AsRef<str> + Serialize, InTuple: Serialize, OutTuple: DeserializeOwned>
    (&mut self, device: HLAPIDeviceHandle, method: Name, args: InTuple) -> IOResult<OutTuple> {
        self.write(&HLAPISend::Invoke {
            device_id: device,
            method_name: method,
            parameters: args,
        })?;
        Ok(self.read()?.expect_result().ok_or(IOErrorKind::InvalidData)?)
    }

    pub fn raw_call_streamed<Name: AsRef<str> + Serialize, InTuple: Serialize, OutItem: DeserializeOwned, Function: FnMut(OutItem) -> Result<(), FnError>, FnError>
    (&mut self, device: HLAPIDeviceHandle, method: Name, args: InTuple, function: &mut Function) -> IOResult<usize> {

        struct Nothing;
        impl Display for Nothing { fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result { Ok(()) } }
        fn stream<'de, Deserializer: serde::Deserializer<'de>, Item: Deserialize<'de>, E>(deserializer: Deserializer, function: impl FnMut(Item) -> Result<(), E>) -> Result<usize, Deserializer::Error> {
            struct StreamingVisitor<Fn, T>(usize, Fn, std::marker::PhantomData<fn(T)>);

            impl<'de, Item: Deserialize<'de>, E, Fn: FnMut(Item) -> Result<(), E>> Visitor<'de> for StreamingVisitor<Fn, Item> {
                type Value = usize;

                fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                    // Error: invalid type: ..., expected
                    formatter.write_str("a sequence of ")?;
                    formatter.write_str(std::any::type_name::<Item>())?;
                    formatter.write_str("'s")?;
                    Ok(())
                }

                fn visit_unit<E>(self) -> Result<Self::Value, E> { // In case of a None
                    Ok(self.0) // assert self.0 == 0
                }

                fn visit_seq<S: SeqAccess<'de>>(mut self, mut seq: S) -> Result<Self::Value, S::Error> {
                    while let Some(item) = seq.next_element()? {
                        self.0 += 1;
                        (self.1)(item).map_err(|_| S::Error::custom(Nothing))?;
                    }
                    Ok(self.0)
                }
            }

            // Create the visitor and ask the deserializer to drive it. The
            // deserializer will call visitor.visit_seq() if a seq is present in
            // the input data.
            let visitor = StreamingVisitor(0, function, std::marker::PhantomData);
            deserializer.deserialize_seq(visitor)
        }

        self.write(&HLAPISend::Invoke {
            device_id: device,
            method_name: method,
            parameters: args,
        })?;

        self.poller.wait_one()?;
        let mut buffer = StackBufReader::<_, READ_BUF>::new(&mut self.handle);

        Self::check_delim(&mut buffer)?;

        let count = stream(&mut serde_json::Deserializer::from_reader(&mut buffer), function).map_err::<IOError, _>(|_| IOErrorKind::InvalidData.into())?;

        // Don't forget to check for delim only after using up the iterator
        Self::check_delim(&mut buffer)?;

        Ok(count)
    }

    fn check_delim<R: Read>(buffer: &mut R) -> IOResult<()> {
        let mut delim_buf = [0; DELIM.len()];
        let bytes_read = buffer.read(&mut delim_buf)?;
        if bytes_read != DELIM.len() || delim_buf != DELIM {
            Err(IOErrorKind::UnexpectedEof)?
        } else { Ok(()) }
    }

    fn read<OutTuple: DeserializeOwned>(&mut self) -> IOResult<HLAPIReceive<OutTuple>> {
        self.poller.wait_one()?;
        let mut buffer = StackBufReader::<_, READ_BUF>::new(&mut self.handle);

        Self::check_delim(&mut buffer)?;

        let mut deserializer = serde_json::Deserializer::from_reader(&mut buffer);
        let data = HLAPIReceive::<OutTuple>::deserialize(&mut deserializer)?;

        Self::check_delim(&mut buffer)?;

        Ok(data)
    }

    /// Throws ErrorKind::WriteZero if the message is over 4kB (absolute limit for sending from VM to Java)
    fn write<Name: AsRef<str> + Serialize, Tuple: Serialize>(&mut self, data: &HLAPISend<Name, Tuple>) -> IOResult<()> {
        let mut buffer = ArrayVec::<u8, MAX_WRITE>::new();

        // Yields IOErrorKind::WriteZero if we're writing more than the buffer can handle
        buffer.write_all(DELIM)?;
        serde_json::to_writer(&mut buffer, data).map_err::<IOError, _>(|_| IOErrorKind::InvalidData.into())?;
        buffer.write_all(DELIM)?;

        // Does not write to the socket, unless the buffer is not overflown, so no need to handle the WriteZero error and flush the bus
        self.handle.write_all(&buffer)?;
        self.handle.flush()?;

        Ok(())
    }

    /// Sends DELIM back into the socket, it makes so on the Java side, it clears the buffer, effectively resetting the state
    pub fn reset(&mut self) -> IOResult<()> {
        self.handle.write_all(DELIM)?;
        self.handle.flush()?;
        Ok(())
    }
}