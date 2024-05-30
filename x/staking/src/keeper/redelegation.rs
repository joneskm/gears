pub use super::*;
use crate::{
    consts::error::TIMESTAMP_NANOS_EXPECT, length_prefixed_addr_pair_key, RedelegationEntry,
};
use gears::store::database::ext::UnwrapCorrupt;

impl<
        SK: StoreKey,
        PSK: ParamsSubspaceKey,
        AK: AccountKeeper<SK>,
        BK: BankKeeper<SK>,
        KH: KeeperHooks<SK>,
    > Keeper<SK, PSK, AK, BK, KH>
{
    /// begin unbonding / redelegation; create a redelegation record
    pub fn begin_redelegation<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        del_addr: &AccAddress,
        val_src_addr: &ValAddress,
        val_dst_addr: &ValAddress,
        shares: Decimal256,
    ) -> anyhow::Result<Timestamp> {
        if val_src_addr == val_dst_addr {
            return Err(AppError::Custom("self redelegation".to_string()).into());
        }

        let mut dst_validator = if let Some(validator) = self.validator(ctx, val_dst_addr) {
            validator
        } else {
            return Err(AppError::Custom(format!("bad redelegation dst: {}", val_dst_addr)).into());
        };

        let src_validator = if let Some(validator) = self.validator(ctx, val_src_addr) {
            validator
        } else {
            return Err(AppError::Custom(format!("bad redelegation src: {}", val_dst_addr)).into());
        };

        // check if this is a transitive redelegation
        if self.has_receiving_redelegation(ctx, del_addr, val_src_addr) {
            return Err(AppError::Custom("transitive redelegation".to_string()).into());
        }

        if self.has_max_redelegation_entries(ctx, del_addr, val_src_addr, val_dst_addr) {
            return Err(AppError::Custom("max redelegation entries".to_string()).into());
        }

        let return_amount = self.unbond(ctx, del_addr, val_src_addr, shares)?;

        if return_amount.is_zero() {
            return Err(AppError::Custom("tiny redelegation amount".to_string()).into());
        }

        let shares_created = self.delegate(
            ctx,
            del_addr,
            return_amount,
            src_validator.status,
            &mut dst_validator,
            false,
        )?;

        // create the unbonding delegation
        let (completion_time, height, complete_now) = self.begin_info(ctx, val_src_addr);
        if complete_now {
            // no need to create the redelegation object
            return Ok(completion_time);
        }

        let redelegation = self.set_redelegation_entry(
            ctx,
            del_addr,
            val_src_addr,
            val_dst_addr,
            height,
            completion_time.clone(),
            return_amount,
            shares_created,
        );

        self.insert_redelegation_queue(ctx, &redelegation, completion_time.clone());
        Ok(completion_time)
    }

    pub fn has_receiving_redelegation<DB: Database, CTX: QueryableContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        del_addr: &AccAddress,
        val_src_addr: &ValAddress,
    ) -> bool {
        let store = ctx.kv_store(&self.store_key);

        let mut prefix = REDELEGATION_BY_VAL_DST_INDEX_KEY.to_vec();
        let postfix = length_prefixed_addr_pair_key(val_src_addr, del_addr);
        prefix.extend_from_slice(&postfix);

        // TODO: check logic
        store.get(&prefix).is_some()
    }

    pub fn has_max_redelegation_entries<DB: Database, CTX: QueryableContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        del_addr: &AccAddress,
        val_src_addr: &ValAddress,
        val_dst_addr: &ValAddress,
    ) -> bool {
        let params = self.staking_params_keeper.get(ctx);

        if let Some(redelegation) = self.redelegation(ctx, del_addr, val_src_addr, val_dst_addr) {
            redelegation.entries.len() >= params.max_entries as usize
        } else {
            false
        }
    }

    /// set_redelegation_entry adds an entry to the unbonding delegation at
    /// the given addresses. It creates the unbonding delegation if it does not exist
    // TODO: consider to change signature
    #[allow(clippy::too_many_arguments)]
    pub fn set_redelegation_entry<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        del_addr: &AccAddress,
        val_src_addr: &ValAddress,
        val_dst_addr: &ValAddress,
        creation_height: u64,
        min_time: Timestamp,
        balance: Uint256,
        shares_dst: Decimal256,
    ) -> Redelegation {
        let entry = RedelegationEntry {
            creation_height,
            completion_time: min_time,
            initial_balance: balance,
            share_dst: shares_dst,
        };
        let redelegation = if let Some(mut redelegation) =
            self.redelegation(ctx, del_addr, val_src_addr, val_dst_addr)
        {
            redelegation.add_entry(entry);
            redelegation
        } else {
            Redelegation {
                delegator_address: del_addr.clone(),
                validator_src_address: val_src_addr.clone(),
                validator_dst_address: val_dst_addr.clone(),
                entries: vec![entry],
            }
        };

        self.set_redelegation(ctx, &redelegation);
        redelegation
    }

    pub fn redelegation<DB: Database, CTX: QueryableContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        del_addr: &AccAddress,
        val_src_addr: &ValAddress,
        val_dst_addr: &ValAddress,
    ) -> Option<Redelegation> {
        let store = ctx.kv_store(&self.store_key);
        let store = store.prefix_store(REDELEGATIONS_KEY);
        let mut key = del_addr.to_string().as_bytes().to_vec();
        key.put(val_src_addr.to_string().as_bytes());
        key.put(val_dst_addr.to_string().as_bytes());
        store
            .get(&key)
            .map(|bytes| serde_json::from_slice(&bytes).unwrap_or_corrupt())
    }

    pub fn set_redelegation<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        delegation: &Redelegation,
    ) {
        let store = ctx.kv_store_mut(&self.store_key);
        let mut delegations_store = store.prefix_store_mut(REDELEGATIONS_KEY);
        let mut key = delegation.delegator_address.to_string().as_bytes().to_vec();
        key.put(delegation.validator_src_address.to_string().as_bytes());
        key.put(delegation.validator_dst_address.to_string().as_bytes());
        delegations_store.set(
            key,
            serde_json::to_vec(&delegation).expect(SERDE_ENCODING_DOMAIN_TYPE),
        );
    }

    pub fn remove_redelegation<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        delegation: &Redelegation,
    ) -> Option<Vec<u8>> {
        let store = ctx.kv_store_mut(&self.store_key);
        let mut delegations_store = store.prefix_store_mut(REDELEGATIONS_KEY);
        let mut key = delegation.delegator_address.to_string().as_bytes().to_vec();
        key.put(delegation.validator_src_address.to_string().as_bytes());
        key.put(delegation.validator_dst_address.to_string().as_bytes());
        delegations_store.delete(&key)
    }

    pub fn complete_redelegation<DB: Database>(
        &self,
        ctx: &mut BlockContext<'_, DB, SK>,
        del_addr: AccAddress,
        val_src_addr: ValAddress,
        val_dst_addr: ValAddress,
    ) -> anyhow::Result<Vec<Coin>> {
        let redelegation = self
            .redelegation(ctx, &del_addr, &val_src_addr, &val_dst_addr)
            .ok_or(AppError::Custom("no redelegation found".to_string()))?;

        let mut balances = vec![];
        let params = self.staking_params_keeper.get(ctx);
        let denom = params.bond_denom;
        let ctx_time = ctx.header.time.clone();
        // TODO: consider to move the DataTime type and work with timestamps into Gears
        // The timestamp is provided by context and conversion won't fail.
        let ctx_time =
            chrono::DateTime::from_timestamp(ctx_time.seconds, ctx_time.nanos as u32).unwrap();

        // loop through all the entries and complete mature redelegation entries
        let mut new_redelegations = vec![];
        for entry in &redelegation.entries {
            let coin = Coin {
                denom: denom.clone(),
                amount: entry.initial_balance,
            };
            if entry.is_mature(ctx_time) && !coin.amount.is_zero() {
                balances.push(coin);
            } else {
                new_redelegations.push(entry);
            }
        }

        // set the redelegation or remove it if there are no more entries
        if new_redelegations.is_empty() {
            self.remove_redelegation(ctx, &redelegation);
        } else {
            self.set_redelegation(ctx, &redelegation);
        }
        Ok(balances)
    }

    pub fn insert_redelegation_queue<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        redelegation: &Redelegation,
        completion_time: Timestamp,
    ) {
        // TODO: consider to move the DataTime type and work with timestamps into Gears
        // The timestamp is provided by context and conversion won't fail.
        let completion_time =
            chrono::DateTime::from_timestamp(completion_time.seconds, completion_time.nanos as u32)
                .unwrap();
        let mut time_slice = self.redelegation_queue_time_slice(ctx, completion_time);
        let dvv_triplet = DvvTriplet {
            del_addr: redelegation.delegator_address.clone(),
            val_src_addr: redelegation.validator_src_address.clone(),
            val_dst_addr: redelegation.validator_dst_address.clone(),
        };
        if time_slice.is_empty() {
            self.set_redelegation_queue_time_slice(ctx, completion_time, vec![dvv_triplet]);
        } else {
            time_slice.push(dvv_triplet);
            self.set_redelegation_queue_time_slice(ctx, completion_time, time_slice);
        }
    }

    pub fn redelegation_queue_time_slice<DB: Database, CTX: QueryableContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        completion_time: chrono::DateTime<Utc>,
    ) -> Vec<DvvTriplet> {
        let store = ctx.kv_store(&self.store_key);
        let store = store.prefix_store(REDELEGATION_QUEUE_KEY);

        let key = completion_time
            .timestamp_nanos_opt()
            .expect(TIMESTAMP_NANOS_EXPECT)
            .to_ne_bytes();
        if let Some(bytes) = store.get(&key) {
            serde_json::from_slice(&bytes).unwrap_or_corrupt()
        } else {
            vec![]
        }
    }

    pub fn set_redelegation_queue_time_slice<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        completion_time: chrono::DateTime<Utc>,
        redelegations: Vec<DvvTriplet>,
    ) {
        let store = ctx.kv_store_mut(&self.store_key);
        let mut store = store.prefix_store_mut(REDELEGATION_QUEUE_KEY);

        let key = completion_time
            .timestamp_nanos_opt()
            .expect(TIMESTAMP_NANOS_EXPECT)
            .to_ne_bytes();
        let value = serde_json::to_vec(&redelegations).expect(SERDE_ENCODING_DOMAIN_TYPE);
        store.set(key, value);
    }

    /// Returns a concatenated list of all the timeslices inclusively previous to
    /// currTime, and deletes the timeslices from the queue
    pub fn dequeue_all_mature_redelegation_queue<
        DB: Database,
        CTX: TransactionalContext<DB, SK>,
    >(
        &self,
        ctx: &mut CTX,
        time: Timestamp,
    ) -> Vec<DvvTriplet> {
        let (keys, mature_redelegations) = {
            let storage = ctx.kv_store(&self.store_key);
            let store = storage.prefix_store(REDELEGATION_QUEUE_KEY);

            // TODO: consider to move the DataTime type and work with timestamps into Gears
            // The timestamp is provided by context and conversion won't fail.
            let time = chrono::DateTime::from_timestamp(time.seconds, time.nanos as u32).unwrap();
            // gets an iterator for all timeslices from time 0 until the current Blockheader time
            let end = unbonding_delegation_time_key(time).to_vec();
            let mut mature_redelegations = vec![];
            let mut keys = vec![];
            // gets an iterator for all timeslices from time 0 until the current Blockheader time
            let mut previous_was_end = false;
            for (k, v) in store.range(..).take_while(|(k, _)| {
                let is_not_end = **k != end;
                let res = is_not_end && !previous_was_end;
                previous_was_end = !is_not_end;
                res
            }) {
                let time_slice: Vec<DvvTriplet> = serde_json::from_slice(&v).unwrap_or_corrupt();
                mature_redelegations.extend(time_slice);
                keys.push(k.to_vec());
            }
            (keys, mature_redelegations)
        };

        let storage = ctx.kv_store_mut(&self.store_key);
        let mut store = storage.prefix_store_mut(UNBONDING_QUEUE_KEY);
        keys.iter().for_each(|k| {
            store.delete(k);
        });
        mature_redelegations
    }
}
