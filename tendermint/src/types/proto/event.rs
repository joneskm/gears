use bytes::Bytes;

#[derive(Clone, PartialEq, Eq, ::prost::Message, serde::Serialize, serde::Deserialize)]
pub struct Event {
    #[prost(string, tag = "1")]
    pub r#type: String,
    #[prost(message, repeated, tag = "2")]
    pub attributes: Vec<EventAttribute>,
}

impl From<inner::Event> for Event {
    fn from(inner::Event { r#type, attributes }: inner::Event) -> Self {
        Self {
            r#type,
            attributes: attributes.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<Event> for inner::Event {
    fn from(Event { r#type, attributes }: Event) -> Self {
        Self {
            r#type,
            attributes: attributes.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, ::prost::Message, serde::Serialize, serde::Deserialize)]
pub struct EventAttribute {
    #[prost(bytes = "bytes", tag = "1")]
    pub key: Bytes,
    #[prost(bytes = "bytes", tag = "2")]
    pub value: Bytes,
    /// nondeterministic
    #[prost(bool, tag = "3")]
    pub index: bool,
}

impl From<inner::EventAttribute> for EventAttribute {
    fn from(inner::EventAttribute { key, value, index }: inner::EventAttribute) -> Self {
        Self { key, value, index }
    }
}

impl From<EventAttribute> for inner::EventAttribute {
    fn from(EventAttribute { key, value, index }: EventAttribute) -> Self {
        Self { key, value, index }
    }
}

pub(crate) mod inner {
    pub use tendermint_proto::abci::Event;
    pub use tendermint_proto::abci::EventAttribute;
}
