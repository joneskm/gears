use std::{borrow::Cow, collections::BTreeMap, ops::RangeBounds};

use database::Database;
use trees::iavl::Tree;

use crate::{
    error::StoreError, utils::MergedRange, QueryableKVStore, TransactionalKVStore, TREE_CACHE_SIZE,
};

use super::prefix::{immutable::ImmutablePrefixStore, mutable::MutablePrefixStore};

#[derive(Debug)]
pub struct KVStore<DB> {
    pub(crate) persistent_store: Tree<DB>,
    block_cache: BTreeMap<Vec<u8>, Vec<u8>>,
    tx_cache: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl<DB: Database> TransactionalKVStore<DB> for KVStore<DB> {
    fn prefix_store_mut(
        &mut self,
        prefix: impl IntoIterator<Item = u8>,
    ) -> MutablePrefixStore<'_, DB> {
        MutablePrefixStore {
            store: self,
            prefix: prefix.into_iter().collect(),
        }
    }

    fn set<KI: IntoIterator<Item = u8>, VI: IntoIterator<Item = u8>>(
        &mut self,
        key: KI,
        value: VI,
    ) {
        let key: Vec<u8> = key.into_iter().collect();

        if key.is_empty() {
            // TODO: copied from SDK, need to understand why this is needed and maybe create a type which captures the restriction
            panic!("key is empty")
        }

        self.tx_cache.insert(key, value.into_iter().collect());
    }
}

impl<DB: Database> QueryableKVStore<DB> for KVStore<DB> {
    fn get<R: AsRef<[u8]> + ?Sized>(&self, k: &R) -> Option<Vec<u8>> {
        let tx_cache_val = self.tx_cache.get(k.as_ref());

        if tx_cache_val.is_none() {
            let block_cache_val = self.block_cache.get(k.as_ref());

            if block_cache_val.is_none() {
                return self.persistent_store.get(k.as_ref());
            };

            return block_cache_val.cloned();
        }

        tx_cache_val.cloned()
    }

    fn prefix_store<I: IntoIterator<Item = u8>>(&self, prefix: I) -> ImmutablePrefixStore<'_, DB> {
        ImmutablePrefixStore {
            store: self.into(),
            prefix: prefix.into_iter().collect(),
        }
    }

    fn range<R: RangeBounds<Vec<u8>> + Clone>(&self, range: R) -> crate::range::Range<'_, R, DB> {
        let cached_values = {
            let tx_cached_values = self.tx_cache.range(range.clone());
            let mut block_cached_values = self
                .block_cache
                .range(range.clone())
                .collect::<BTreeMap<_, _>>();

            block_cached_values.extend(tx_cached_values);
            block_cached_values
                .into_iter()
                .map(|(first, second)| (Cow::Borrowed(first), Cow::Borrowed(second)))
        };

        let persisted_values = self
            .persistent_store
            .range(range)
            .map(|(first, second)| (Cow::Owned(first), Cow::Owned(second)));

        MergedRange::merge(cached_values, persisted_values).into()
    }

    // fn get_keys(&self, key_prefix: &(impl AsRef<[u8]> + ?Sized)) -> Vec<Vec<u8>> {
    //     self.persistent_store
    //         .range(..)
    //         .map(|(key, _value)| key)
    //         .filter(|key| key.starts_with(key_prefix.as_ref()))
    //         .collect()
    // }
}

impl<DB: Database> KVStore<DB> {
    pub fn new(db: DB, target_version: Option<u32>) -> Result<Self, StoreError> {
        Ok(KVStore {
            persistent_store: Tree::new(
                db,
                target_version,
                TREE_CACHE_SIZE.try_into().expect("tree cache size is > 0"),
            )?,
            block_cache: BTreeMap::new(),
            tx_cache: BTreeMap::new(),
        })
    }

    /// Returns default if failed to save cache
    pub fn commit(&mut self) -> [u8; 32] {
        self.write_then_clear_tx_cache();
        self.write_then_clear_block_cache();
        let (hash, _) = self
            .persistent_store
            .save_version()
            .ok()
            .unwrap_or_default(); //TODO: is it safe to assume this won't ever error?
        hash
    }

    pub fn delete(&mut self, k: &[u8]) -> Option<Vec<u8>> {
        let tx_value = self.tx_cache.remove(k);
        let block_value = self.block_cache.remove(k);
        let persisted_value = self.persistent_store.remove(k);

        tx_value.or(block_value).or(persisted_value)
    }

    /// Writes tx cache into block cache then clears the tx cache
    pub fn write_then_clear_tx_cache(&mut self) {
        let mut keys: Vec<&Vec<u8>> = self.tx_cache.keys().collect();
        keys.sort();

        for key in keys {
            let value = self
                .tx_cache
                .get(key)
                .expect("key is definitely in the HashMap");
            self.block_cache.insert(key.to_owned(), value.to_owned());
        }
        self.tx_cache.clear();
    }

    /// Clears the tx cache
    pub fn clear_tx_cache(&mut self) {
        self.tx_cache.clear();
    }

    /// Writes block cache into the tree store then clears the block cache
    fn write_then_clear_block_cache(&mut self) {
        let mut keys: Vec<&Vec<u8>> = self.block_cache.keys().collect();
        keys.sort();

        for key in keys {
            let value = self
                .block_cache
                .get(key)
                .expect("key is definitely in the HashMap");
            self.persistent_store.set(key.to_owned(), value.to_owned())
        }
        self.block_cache.clear();
    }

    pub fn head_commit_hash(&self) -> [u8; 32] {
        self.persistent_store.root_hash()
    }

    pub fn last_committed_version(&self) -> u32 {
        self.persistent_store.loaded_version()
    }
}

#[cfg(test)]
mod test {
    use std::{borrow::Cow, ops::Bound};

    use database::MemDB;

    use crate::{types::kv::KVStore, QueryableKVStore, TransactionalKVStore};

    /// Tests whether kv range works with cached and persisted values
    #[test]
    fn kv_store_merged_range_works() {
        let db = MemDB::new();
        let mut store = KVStore::new(db, None).unwrap();

        // values in this group will be in the persistent store
        store.set(vec![1], vec![1]);
        store.set(vec![7], vec![13]); // shadowed by value in tx cache
        store.set(vec![10], vec![2]); // shadowed by value in block cache
        store.set(vec![14], vec![234]); // shadowed by value in block cache and tx cache
        store.commit();

        // values in this group will be in the block cache
        store.set(vec![2], vec![3]);
        store.set(vec![9], vec![4]); // shadowed by value in tx cache
        store.set(vec![10], vec![7]); // shadows a persisted value
        store.set(vec![14], vec![212]); // shadows a persisted value AND shadowed by value in tx cache
        store.write_then_clear_tx_cache();

        // values in this group will be in the tx cache
        store.set(vec![3], vec![5]);
        store.set(vec![8], vec![6]);
        store.set(vec![7], vec![5]); // shadows a persisted value
        store.set(vec![9], vec![6]); // shadows a block cache value
        store.set(vec![14], vec![212]); // shadows a persisted value which shadows a persisted value

        let start = vec![0];
        let stop = vec![20];
        let got_pairs = store
            .range((Bound::Excluded(start), Bound::Excluded(stop)))
            .collect::<Vec<_>>();
        let expected_pairs = [
            (vec![1], vec![1]),
            (vec![2], vec![3]),
            (vec![3], vec![5]),
            (vec![7], vec![5]),
            (vec![8], vec![6]),
            (vec![9], vec![6]),
            (vec![10], vec![7]),
            (vec![14], vec![212]),
        ]
        .into_iter()
        .map(|(first, second)| (Cow::Owned(first), Cow::Owned(second)))
        .collect::<Vec<_>>();

        assert_eq!(expected_pairs, got_pairs);
    }
}