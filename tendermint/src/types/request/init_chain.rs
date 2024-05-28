use bytes::Bytes;

use crate::{
    error::Error,
    types::{
        chain_id::ChainId,
        proto::{consensus::ConsensusParams, validator::ValidatorUpdate},
        time::Timestamp,
    },
};

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestInitChain {
    pub time: Timestamp,
    pub chain_id: ChainId,
    pub consensus_params: ConsensusParams,
    pub validators: Vec<ValidatorUpdate>,
    pub app_state_bytes: Bytes,
    pub initial_height: i64, //TODO: use u64?
}

impl From<RequestInitChain> for super::inner::RequestInitChain {
    fn from(
        RequestInitChain {
            time,
            chain_id,
            consensus_params,
            validators,
            app_state_bytes,
            initial_height,
        }: RequestInitChain,
    ) -> Self {
        Self {
            time: Some(time.into()),
            chain_id: chain_id.into(),
            consensus_params: Some(consensus_params.into()),
            validators: validators.into_iter().map(Into::into).collect(),
            app_state_bytes,
            initial_height,
        }
    }
}

impl TryFrom<super::inner::RequestInitChain> for RequestInitChain {
    type Error = Error;

    fn try_from(
        super::inner::RequestInitChain {
            time,
            chain_id,
            consensus_params,
            validators,
            app_state_bytes,
            initial_height,
        }: super::inner::RequestInitChain,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            time: time
                .ok_or(Error::InvalidData("time is empty".to_string()))?
                .into(),
            chain_id: chain_id
                .parse()
                .map_err(|e| Self::Error::InvalidData(format!("invalid chain_id: {e}").into()))?,
            consensus_params: consensus_params
                .ok_or(Error::InvalidData("consensus params is empty".to_string()))?
                .try_into()?,
            validators: validators
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<ValidatorUpdate>, Error>>()?,
            app_state_bytes,
            initial_height,
        })
    }
}
