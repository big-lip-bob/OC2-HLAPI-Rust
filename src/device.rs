use uuid::Uuid;

pub trait HLAPIDevice {
    const IDENTIFIER: &'static str;
    fn uuid(&self) -> Uuid;
}