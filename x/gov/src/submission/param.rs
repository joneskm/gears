use gears::{
    derive::{AppMessage, Protobuf, Raw},
    params::ParamsSubspaceKey,
};
use ibc_proto::google::protobuf::Any;
use prost::Message;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Raw, Protobuf, AppMessage)]
#[raw(derive(Serialize, Deserialize, Clone, PartialEq))]
#[msg(url = "/cosmos.params.v1beta1/ParamChange")]
pub struct ParamChange<PSK: ParamsSubspaceKey> {
    #[raw(kind(string), raw = String)]
    #[proto(
        from = "PSK::from_subspace_str",
        from_ref,
        into = "PSK::name",
        into_ref
    )]
    pub subspace: PSK,
    #[raw(kind(bytes))]
    #[proto(repeated)]
    pub key: Vec<u8>,
    #[raw(kind(bytes))]
    #[proto(repeated)]
    pub value: Vec<u8>,
}

// Serde macro slightly dumb for such cases so I did it myself
impl<PSK: ParamsSubspaceKey> serde::Serialize for ParamChange<PSK> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        RawParamChange::from(self.clone()).serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Raw, Protobuf, AppMessage)]
#[raw(derive(Serialize, Deserialize, Clone, PartialEq))]
#[msg(url = "/cosmos.params.v1beta1/ParameterChangeProposal")]
pub struct ParameterChangeProposal<PSK: ParamsSubspaceKey> {
    #[raw(kind(string), raw = String)]
    pub title: String,
    #[raw(kind(string), raw = String)]
    pub description: String,
    #[raw(kind(message), raw = RawParamChange, repeated)]
    #[proto(repeated)]
    pub changes: Vec<ParamChange<PSK>>,
}

impl<PSK: ParamsSubspaceKey> Serialize for ParameterChangeProposal<PSK> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        RawParameterChangeProposal::from(self.clone()).serialize(serializer)
    }
}

impl From<RawParameterChangeProposal> for Any {
    fn from(msg: RawParameterChangeProposal) -> Self {
        Any {
            type_url: "/cosmos.params.v1beta1/ParameterChangeProposal".to_owned(),
            value: msg.encode_to_vec(),
        }
    }
}
