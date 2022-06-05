use serde::{Serialize, Deserialize};

pub type HLAPIDeviceHandle = uuid::Uuid;

//pub type Empty = [(); 0]; // TODO: verify if this works
#[derive(Serialize, Deserialize)] pub struct Empty { } // used as the empty parameters specifier
pub const EMPTY: Empty = Empty { };
pub const NOTHING: Void = None;

// TODO: Turn the tagged content enum into a generic struct, separating each entry, thus reducing enum size
// Requires some research / code
// generation, see https://canary.discord.com/channels/273534239310479360/274215136414400513/948701733612290131
#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type", content = "data")]
pub enum HLAPISend<Name: AsRef<str> = &'static str, Tuple/*: Serialize*/ = Empty> {
    List,
    Methods (HLAPIDeviceHandle),
    #[serde(rename_all = "camelCase")] // Why dont you propagate to all the enum sub-members ??
    Invoke {
        device_id: HLAPIDeviceHandle, // hyphenated
        #[serde(rename = "name")]
        method_name: Name,
        parameters: Tuple
    }
}

#[derive(Serialize, Deserialize)]
pub enum Never { }
pub type Void = Option<Never>;

#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type", content = "data")]
pub enum HLAPIReceive<Tuple/*: DeserializeOwned */ = Void /*always None!*/> {
    List (Vec<HLAPIDeviceDescriptor>),
    Methods (Vec<HLAPIMethod>),
    Error (Option<String>),
    Result (Tuple) // returned values
}

impl<Tuple> HLAPIReceive<Tuple> {
    pub fn expect_result(self) -> Option<Tuple> { if let Self::Result(tuple) = self { Some(tuple) } else { None } }
}

#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HLAPIDeviceDescriptor {
    pub device_id: HLAPIDeviceHandle,
    #[serde(rename = "typeNames")]
    pub components: Vec<String> // cannot be empty thus no default
}

#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HLAPIMethod {
    pub name: String,
    #[serde(default)] // skip_serializing_if = "Vec::is_empty")
    pub parameters: Vec<HLAPIType>, // Must get respected 1:1
    pub return_type: String, // always here ?

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_value_description: Option<String>
}

#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HLAPIType {
    #[serde(rename = "type")]
    data: String
}