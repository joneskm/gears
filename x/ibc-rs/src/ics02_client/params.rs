use std::collections::HashMap;
use std::collections::HashSet;

use gears::params::subspace;
use gears::params::subspace_mut;
use gears::params::ParamKind;
use gears::params::ParamsDeserialize;
use gears::params::ParamsSerialize;
use gears::params::ParamsSubspaceKey;
use gears::store::QueryableMultiKVStore;
use gears::store::ReadPrefixStore;
use gears::store::TransactionalMultiKVStore;
use gears::store::WritePrefixStore;
use gears::{
    store::{
        database::{prefix::PrefixDB, Database},
        StoreKey,
    },
    types::context::{QueryableContext, TransactionalContext},
};
use serde::{Deserialize, Serialize};

const KEY_ALLOWED_CLIENTS: &str = "AllowedClients";

/// Params defines the set of IBC light client parameters.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ClientParams {
    /// allowed_clients defines the list of allowed client state types which can be created
    /// and interacted with. If a client type is removed from the allowed clients list, usage
    /// of this client will be disabled until it is added again to the list.
    #[prost(string, repeated, tag = "1")]
    pub allowed_clients: Vec<String>,
}

impl ParamsSerialize for ClientParams {
    fn keys() -> HashMap<&'static str, ParamKind> {
        [(KEY_ALLOWED_CLIENTS, ParamKind::Bytes)]
            .into_iter()
            .collect()
    }

    fn to_raw(&self) -> Vec<(&'static str, Vec<u8>)> {
        let mut hash_map = Vec::with_capacity(1);

        hash_map.push((
            KEY_ALLOWED_CLIENTS,
            serde_json::to_vec(&self.allowed_clients).expect("conversion to json won't fail"),
        ));

        hash_map
    }
}

impl ParamsDeserialize for ClientParams {
    fn from_raw(fields: HashMap<&'static str, Vec<u8>>) -> Self {
        Self {
            allowed_clients: serde_json::from_slice(
                fields.get(KEY_ALLOWED_CLIENTS).expect("expected to exists"),
            )
            .expect("conversion from json won't fail"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientParamsKeeper<SK, PSK> {
    pub store_key: SK,
    pub params_subspace_key: PSK,
}

impl<SK: StoreKey, PSK: ParamsSubspaceKey> ClientParamsKeeper<SK, PSK> {
    pub fn get<DB: Database, KV: QueryableContext<DB, SK>>(&self, ctx: &KV) -> ClientParams {
        let store = subspace(ctx, &self.store_key, &self.params_subspace_key);

        store.params().expect("should exists")
    }

    pub fn set<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        params: ClientParams,
    ) {
        let mut store = subspace_mut(ctx, &self.store_key, &self.params_subspace_key);

        store.params_set(&params)
    }
}
