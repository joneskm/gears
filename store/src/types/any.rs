use std::ops::RangeBounds;

use database::Database;

use crate::range::Range;

use super::{kv::commit::CommitKVStore, query::kv::QueryKVStore};

#[derive(Debug)]
pub enum AnyKVStore<'a, DB> {
    KVStore(&'a CommitKVStore<DB>),
    QueryKVStore(&'a QueryKVStore<'a, DB>),
}

impl<'a, DB: Database> From<&'a CommitKVStore<DB>> for AnyKVStore<'a, DB> {
    fn from(kv_store: &'a CommitKVStore<DB>) -> Self {
        Self::KVStore(kv_store)
    }
}

impl<'a, DB: Database> From<&'a QueryKVStore<'a, DB>> for AnyKVStore<'a, DB> {
    fn from(kv_store: &'a QueryKVStore<'a, DB>) -> Self {
        Self::QueryKVStore(kv_store)
    }
}

impl<'a, DB: Database> AnyKVStore<'a, DB> {
    pub fn get(&self, k: &impl AsRef<[u8]>) -> Option<Vec<u8>> {
        match self {
            AnyKVStore::KVStore(store) => store.get(k),
            AnyKVStore::QueryKVStore(store) => store.get(k),
        }
    }

    pub fn range<R>(&self, range: R) -> Range<'_, R, DB>
    where
        R: RangeBounds<Vec<u8>> + Clone,
    {
        match self {
            AnyKVStore::KVStore(store) => store.range(range),
            AnyKVStore::QueryKVStore(store) => store.range(range),
        }
    }
}