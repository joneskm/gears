use ibc_proto::cosmos::bank::v1beta1::DenomUnit as RawDenomUnit;
use ibc_proto::cosmos::bank::v1beta1::Metadata as RawMetadata;
use nutype::nutype;
use prost::bytes::Bytes;
use prost::Message;
use proto_types::Denom;

use crate::Error;

pub struct DenomUnit {
    pub denom: Denom,
    pub exponent: u32,
    pub aliases: Vec<String>,
}

impl TryFrom<RawDenomUnit> for DenomUnit {
    type Error = proto_types::Error;

    fn try_from(value: RawDenomUnit) -> Result<Self, Self::Error> {
        let RawDenomUnit {
            denom,
            exponent,
            aliases,
        } = value;

        Ok(Self {
            denom: Denom::try_from(denom)?,
            exponent,
            aliases,
        })
    }
}

impl From<DenomUnit> for RawDenomUnit {
    fn from(value: DenomUnit) -> Self {
        let DenomUnit {
            denom,
            exponent,
            aliases,
        } = value;

        Self {
            denom: denom.into_inner(),
            exponent,
            aliases,
        }
    }
}

#[nutype(validate(not_empty), derive( Debug, Clone, Deserialize, Serialize))]
pub struct UriHash(String);

pub struct Metadata {
    pub description: String,
    /// denom_units represents the list of DenomUnit's for a given coin
    pub denom_units: Vec<DenomUnit>,
    /// base represents the base denom (should be the DenomUnit with exponent = 0).
    pub base: String,
    /// display indicates the suggested denom that should be
    /// displayed in clients.
    pub display: String,
    /// name defines the name of the token (eg: Cosmos Atom)
    pub name: String,
    /// symbol is the token symbol usually shown on exchanges (eg: ATOM). This can
    /// be the same as the display.
    pub symbol: String,
    /// URI to a document (on or off-chain) that contains additional information. Optional.
    pub uri: String,
    /// URIHash is a sha256 hash of a document pointed by URI. It's used to verify that
    /// the document didn't change. Optional.
    pub uri_hash: Option<UriHash>,
}

impl Metadata {
    pub fn from_bytes(raw: Bytes) -> Result<Self, Error> {
        let meta = RawMetadata::decode(raw)?;

        meta.try_into()
    }
}

impl TryFrom<RawMetadata> for Metadata {
    type Error = Error;

    fn try_from(value: RawMetadata) -> Result<Self, Self::Error> {
        let RawMetadata {
            description,
            denom_units,
            base,
            display,
            name,
            symbol,
            uri,
            uri_hash,
        } = value;

        let mut mapped_denom: Vec<DenomUnit> = Vec::with_capacity(denom_units.len());
        for unit in denom_units {
            mapped_denom.push(
                DenomUnit::try_from(unit)
                    .map_err(|e: proto_types::Error| Error::Custom(e.to_string()))?,
            );
        }

        Ok(Self {
            description,
            denom_units: mapped_denom,
            base,
            display,
            name,
            symbol,
            uri,
            uri_hash: {
                if uri_hash.is_empty() {
                    None
                } else {
                    Some(UriHash::new(uri_hash).map_err(|e| Error::Custom(e.to_string()))?)
                }
            },
        })
    }
}

impl From<Metadata> for RawMetadata {
    fn from(value: Metadata) -> Self {
        let Metadata {
            description,
            denom_units,
            base,
            display,
            name,
            symbol,
            uri,
            uri_hash,
        } = value;

        let uri_hash = if let Some(uri) = uri_hash {
            uri.into_inner()
        } else {
            String::new()
        };

        Self {
            description,
            denom_units: denom_units.into_iter().map(RawDenomUnit::from).collect(),
            base,
            display,
            name,
            symbol,
            uri,
            uri_hash,
        }
    }
}