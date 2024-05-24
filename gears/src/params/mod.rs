use std::{collections::HashMap, hash::Hash, str::FromStr};

use database::{prefix::PrefixDB, Database};
use store_crate::{QueryableKVStore, StoreKey, TransactionalKVStore};

use crate::types::context::{QueryableContext, TransactionalContext};

use self::{parsed::Params, space::ParamsSpace, space_mut::ParamsSpaceMut};

pub mod parsed;
pub mod space;
pub mod space_mut;

pub fn subspace<
    'a,
    DB: Database,
    SK: StoreKey,
    CTX: QueryableContext<DB, SK>,
    PSK: ParamsSubspaceKey,
>(
    ctx: &'a CTX,
    store_key: &SK,
    params_subspace_key: &PSK,
) -> ParamsSpace<'a, PrefixDB<DB>> {
    ParamsSpace {
        inner: ctx
            .kv_store(store_key)
            .prefix_store(params_subspace_key.name().as_bytes().to_vec()),
    }
}

pub fn subspace_mut<
    'a,
    DB: Database,
    SK: StoreKey,
    CTX: TransactionalContext<DB, SK>,
    PSK: ParamsSubspaceKey,
>(
    ctx: &'a mut CTX,
    store_key: &SK,
    params_subspace_key: &PSK,
) -> ParamsSpaceMut<'a, PrefixDB<DB>> {
    ParamsSpaceMut {
        inner: ctx
            .kv_store_mut(store_key)
            .prefix_store_mut(params_subspace_key.name().as_bytes().to_vec()),
    }
}

pub trait ParamsSubspaceKey: Hash + Eq + Clone + Send + Sync + 'static {
    fn name(&self) -> &'static str;
}

// TODO:LATER For PR with xmod to change any params
// pub trait ModuleParams {
//     fn module_params<PSK: ParamsSubspaceKey, P: Params>() -> (PSK, P);
// }

pub trait ParamsSerialize {
    /// Return all unique keys for this structure
    fn keys() -> HashMap<&'static str, ParamKind>;
    fn to_raw(&self) -> Vec<(&'static str, Vec<u8>)>;
}

pub trait ParamsDeserialize: ParamsSerialize {
    fn from_raw(fields: HashMap<&'static str, Vec<u8>>) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParamKind {
    Bytes,
    String,
    Bool,
    U64,
    I64,
    U32,
    I32,
    U16,
    I16,
    U8,
    I8,
}

impl ParamKind {
    pub fn parse_param(self, bytes: Vec<u8>) -> Params {
        fn parse_primitive_bytes<T: FromStr>(value: Vec<u8>) -> T
        where
            <T as FromStr>::Err: std::fmt::Debug,
        {
            String::from_utf8(value)
                .expect("should be valid utf-8")
                .strip_suffix('\"')
                .unwrap() // TODO
                .strip_prefix('\"')
                .unwrap() // TODO
                .to_owned()
                .parse()
                .unwrap() // TODO
        }

        match self {
            ParamKind::Bytes => Params::Bytes(bytes),
            ParamKind::String => match String::from_utf8(bytes) {
                Ok(var) => Params::String(var),
                Err(err) => Params::InvalidCast(err.into_bytes()),
            },
            ParamKind::Bool => match bool::from_str(&String::from_utf8_lossy(&bytes)) {
                Ok(var) => Params::Bool(var),
                Err(_) => Params::InvalidCast(bytes),
            },
            ParamKind::U64 => Params::U64(parse_primitive_bytes(bytes)),
            ParamKind::I64 => Params::I64(parse_primitive_bytes(bytes)),
            ParamKind::U32 => Params::U32(parse_primitive_bytes(bytes)),
            ParamKind::I32 => Params::I32(parse_primitive_bytes(bytes)),
            ParamKind::U16 => Params::U16(parse_primitive_bytes(bytes)),
            ParamKind::I16 => Params::I16(parse_primitive_bytes(bytes)),
            ParamKind::U8 => Params::U8(parse_primitive_bytes(bytes)),
            ParamKind::I8 => Params::I8(parse_primitive_bytes(bytes)),
        }
    }
}
