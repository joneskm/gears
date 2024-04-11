use store_crate::database::{Database, PrefixDB};
use store_crate::{
    types::{kv::KVStore, multi::MultiStore},
    ReadMultiKVStore, StoreKey, WriteMultiKVStore,
};
use tendermint::types::{
    chain_id::ChainId,
    proto::{event::Event, header::Header},
};

use super::{QueryableContext, TransactionalContext};

pub struct TxContext<'a, DB, SK> {
    multi_store: &'a mut MultiStore<DB, SK>,
    pub height: u64,
    pub events: Vec<Event>,
    pub header: Header,
    _tx_bytes: Vec<u8>,
    pub chain_id: ChainId,
}

impl<'a, DB: Database, SK: StoreKey> TxContext<'a, DB, SK> {
    pub fn new(
        multi_store: &'a mut MultiStore<DB, SK>,
        height: u64,
        header: Header,
        tx_bytes: Vec<u8>,
    ) -> Self {
        TxContext {
            multi_store,
            height,
            events: vec![],
            header,
            _tx_bytes: tx_bytes,
            chain_id: ChainId::new("todo-900").expect("Unrechable. Default should be valid"),
        }
    }
}

impl<DB: Database, SK: StoreKey> QueryableContext<PrefixDB<DB>, SK> for TxContext<'_, DB, SK> {
    type KVStore = KVStore<PrefixDB<DB>>;

    fn kv_store(&self, store_key: &SK) -> &Self::KVStore {
        self.multi_store.kv_store(store_key)
    }

    fn height(&self) -> u64 {
        self.height
    }

    fn chain_id(&self) -> &ChainId {
        &self.chain_id
    }
}

impl<DB: Database, SK: StoreKey> TransactionalContext<PrefixDB<DB>, SK> for TxContext<'_, DB, SK> {
    type KVStoreMut = KVStore<PrefixDB<DB>>;

    fn kv_store_mut(&mut self, store_key: &SK) -> &mut Self::KVStoreMut {
        self.multi_store.kv_store_mut(store_key)
    }

    fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }

    fn append_events(&mut self, mut events: Vec<Event>) {
        self.events.append(&mut events);
    }
}
