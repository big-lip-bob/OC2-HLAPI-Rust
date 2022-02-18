use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::io::{Result as IOResult, Error as IOError, ErrorKind as IOErrorKind, Write, Read};
use std::time::Duration;
use epoll_rs::{Epoll, Opts as PollOpts};
use serde::{Serialize, Deserialize};
use serde::{ser::Serialize as SerializeOwned,de::DeserializeOwned};
use stack_buffer::{StackBufReader, StackBufWriter};
use uuid::Uuid;

/// Used as the delimiter for HLAPI JSON packets
const DELIM: &[u8] = b"\0";

/// Used in the Lua implementation
const BUF_SIZE: usize = 1024;

/// Main bus path
const MAIN_BUS: &str = "/dev/hvc0";

pub struct HLAPIBus {
    handle: File,
    poller: Epoll
}

pub type HLAPIDevice = Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type", content = "data")]
pub enum HLAPISend {
    List,
    Methods (HLAPIDevice),
    Invoke {
        device_id: HLAPIDevice, // hyphenated
        #[serde(rename = "name")]
        method_name: String,
        parameters: Vec<serde_json::Value>
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type", content = "data")]
pub enum HLAPIReceive {
    List (Vec<HLAPIDeviceDescriptor>),
    Methods (Vec<HLAPIMethod>),
    Error (String),
    Result (Vec<String>) // returned values
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HLAPIDeviceDescriptor {
    pub device_id: HLAPIDevice,
    #[serde(rename = "typeNames", default)]
    pub components: Vec<String>
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HLAPIMethod {
    pub name: String,
    #[serde(default)]
    pub parameters: Vec<String>, // Must get respected 1:1
    pub return_type: Option<String>,

    pub description: Option<String>,
    pub return_value_description: Option<String>
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

        termios::cfsetspeed(&mut termios, termios::B38400)?; // baud 38400

        Ok(Self { handle, poller })
    }

    pub fn list(&mut self) -> IOResult<Vec<HLAPIDeviceDescriptor>> {
        self.write(&HLAPISend::List)?;
        let list: HLAPIReceive = self.read()?;
        if let HLAPIReceive::List(devices) = list {
            Ok(devices)
        } else { Err(IOErrorKind::InvalidData.into()) }
    }

    pub fn methods(&mut self, device: HLAPIDevice) -> IOResult<Vec<HLAPIMethod>> {
        self.write(&HLAPISend::Methods(device))?;
        let list: HLAPIReceive = self.read()?;
        if let HLAPIReceive::Methods(methods) = list {
            Ok(methods)
        } else { Err(IOErrorKind::InvalidData.into()) }
    }

    pub fn find(&mut self, name: &str) -> IOResult<HLAPIDevice> {
        for HLAPIDeviceDescriptor { device_id, components } in self.list()? {
            if components.into_iter().any(|dev| name == dev) { return Ok(device_id); }
        }
        Err(IOErrorKind::NotFound.into())
    }

    fn write<T: SerializeOwned>(&mut self, data: &T) -> IOResult<()> {
        let mut buffer = StackBufWriter::<_, BUF_SIZE>::new(&mut self.handle);

        buffer.write_all(DELIM)?;
        serde_json::to_writer(&mut buffer, data).map_err::<IOError, _>(|_| IOErrorKind::InvalidData.into())?;
        buffer.write_all(DELIM)?;

        buffer.flush()?;

        Ok(())
    }

    fn check_null<R: Read>(buffer: &mut R) -> IOResult<()> { // Unexpected EOF
        let mut must_null = [0; 1];
        let n = buffer.read(&mut must_null)?;
        if n == 0 || must_null[0] != 0 { Err(IOErrorKind::UnexpectedEof)? } else { Ok(()) }
    }

    fn read<T: DeserializeOwned>(&mut self) -> IOResult<T> {
        self.poller.wait_one()?;
        let mut buffer = StackBufReader::<_, BUF_SIZE>::new(&mut self.handle);

        Self::check_null(&mut buffer)?;

        let mut deserializer = serde_json::Deserializer::from_reader(&mut buffer);
        let data = T::deserialize(&mut deserializer)?;

        Self::check_null(&mut buffer)?;

        Ok(data)
    }

}