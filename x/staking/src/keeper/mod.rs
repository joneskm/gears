use crate::{
    BondStatus, Delegation, DvPair, DvvTriplet, GenesisState, LastValidatorPower, Pool,
    Redelegation, StakingParamsKeeper, UnbondingDelegation, Validator,
};
use chrono::Utc;
use gears::{
    core::address::{AccAddress, ValAddress},
    crypto::keys::ReadAccAddress,
    error::AppError,
    params::ParamsSubspaceKey,
    store::{
        database::Database, QueryableKVStore, ReadPrefixStore, StoreKey, TransactionalKVStore,
        WritePrefixStore,
    },
    tendermint::types::proto::{
        event::{Event, EventAttribute},
        validator::ValidatorUpdate,
    },
    types::{
        base::{coin::Coin, send::SendCoins},
        context::{init::InitContext, QueryableContext, TransactionalContext},
        decimal256::Decimal256,
        uint::Uint256,
    },
    x::keepers::auth::AuthKeeper,
};
use prost::{bytes::BufMut, Message};
use serde::de::Error;
use std::{cmp::Ordering, collections::HashMap};

// Each module contains methods of keeper with logic related to its name. It can be delegation and
// validator types.

mod bonded;
mod delegation;
mod hooks;
mod redelegation;
mod traits;
mod unbonded;
mod unbonding;
mod validator;
mod validators_and_total_power;
pub use traits::*;
use unbonding::*;
use validator::*;

const POOL_KEY: [u8; 1] = [0];
const LAST_TOTAL_POWER_KEY: [u8; 1] = [1];
const VALIDATORS_KEY: [u8; 1] = [2];
const LAST_VALIDATOR_POWER_KEY: [u8; 1] = [3];
const DELEGATIONS_KEY: [u8; 1] = [4];
const REDELEGATIONS_KEY: [u8; 1] = [5];
pub(crate) const VALIDATORS_BY_POWER_INDEX_KEY: [u8; 1] = [6];
const VALIDATORS_QUEUE_KEY: [u8; 1] = [7];
const UBD_QUEUE_KEY: [u8; 1] = [8];
const UNBONDING_QUEUE_KEY: [u8; 1] = [9];
const REDELEGATION_QUEUE_KEY: [u8; 1] = [10];

const NOT_BONDED_POOL_NAME: &str = "not_bonded_tokens_pool";
const BONDED_POOL_NAME: &str = "bonded_tokens_pool";
const EVENT_TYPE_COMPLETE_UNBONDING: &str = "complete_unbonding";
const EVENT_TYPE_COMPLETE_REDELEGATION: &str = "complete_redelegation";
const ATTRIBUTE_KEY_AMOUNT: &str = "amount";
const ATTRIBUTE_KEY_VALIDATOR: &str = "validator";
const ATTRIBUTE_KEY_DELEGATOR: &str = "delegator";

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Keeper<
    SK: StoreKey,
    PSK: ParamsSubspaceKey,
    AK: AccountKeeper<SK>,
    BK: BankKeeper<SK>,
    KH: KeeperHooks<SK>,
> {
    store_key: SK,
    auth_keeper: AK,
    bank_keeper: BK,
    staking_params_keeper: StakingParamsKeeper<SK, PSK>,
    codespace: String,
    hooks_keeper: Option<KH>,
}

impl<
        SK: StoreKey,
        PSK: ParamsSubspaceKey,
        AK: AccountKeeper<SK>,
        BK: BankKeeper<SK>,
        KH: KeeperHooks<SK>,
    > Keeper<SK, PSK, AK, BK, KH>
{
    pub fn new(
        store_key: SK,
        params_subspace_key: PSK,
        auth_keeper: AK,
        bank_keeper: BK,
        params_keeper: gears::params::Keeper<SK, PSK>,
        codespace: String,
    ) -> Self {
        let staking_params_keeper = StakingParamsKeeper {
            params_keeper,
            params_subspace_key,
        };

        Keeper {
            store_key,
            auth_keeper,
            bank_keeper,
            staking_params_keeper,
            codespace,
            hooks_keeper: None,
        }
    }

    pub fn init_genesis<DB: Database>(
        &self,
        ctx: &mut InitContext<'_, DB, SK>,
        genesis: GenesisState,
    ) -> anyhow::Result<Vec<ValidatorUpdate>> {
        let mut bonded_tokens = Uint256::zero();
        let mut not_bonded_tokens = Uint256::zero();

        // TODO
        // ctx = ctx.WithBlockHeight(1 - sdk.ValidatorUpdateDelay)

        self.set_pool(ctx, genesis.pool)?;
        self.set_last_total_power(ctx, genesis.last_total_power);
        self.staking_params_keeper
            .set(&mut ctx.multi_store_mut(), genesis.params.clone())?;

        for validator in genesis.validators {
            self.set_validator(ctx, &validator)?;
            // Manually set indices for the first time
            self.set_validator_by_cons_addr(ctx, &validator)?;
            self.set_validator_by_power_index(ctx, &validator)?;

            if !genesis.exported {
                self.after_validator_created(ctx, &validator)?;
            }

            if validator.status == BondStatus::Unbonding {
                self.insert_unbonding_validator_queue(ctx, &validator)?;
            }

            match validator.status {
                BondStatus::Bonded => {
                    bonded_tokens += validator.tokens.amount;
                }
                BondStatus::Unbonding | BondStatus::Unbonded => {
                    not_bonded_tokens += validator.tokens.amount;
                }
            }
        }

        for delegation in genesis.delegations {
            if !genesis.exported {
                self.before_delegation_created(ctx, &delegation)?;
            }

            self.set_delegation(ctx, &delegation)?;

            if !genesis.exported {
                self.after_delegation_modified(ctx, &delegation)?;
            }
        }

        for unbonding_delegation in genesis.unbonding_delegations {
            self.set_unbonding_delegation(ctx, &unbonding_delegation)?;
            for entry in unbonding_delegation.entries.as_slice() {
                self.insert_ubd_queue(ctx, &unbonding_delegation, entry.completion_time)?;
            }
        }

        for redelegation in genesis.redelegations {
            self.set_redelegation(ctx, &redelegation)?;
            for entry in &redelegation.entries {
                self.insert_redelegation_queue(ctx, &redelegation, entry.completion_time)?;
            }
        }

        let bonded_coins = SendCoins::new(vec![Coin {
            denom: genesis.params.bond_denom.clone(),
            amount: bonded_tokens,
        }])?;
        let not_bonded_coins = SendCoins::new(vec![Coin {
            denom: genesis.params.bond_denom,
            amount: not_bonded_tokens,
        }])?;

        // check if the unbonded and bonded pools accounts exists
        let bonded_pool = self.get_bonded_pool(ctx).ok_or(AppError::Custom(format!(
            "{} module account has not been set",
            BONDED_POOL_NAME
        )))?;

        // TODO: check cosmos issue
        let bonded_balance = self
            .bank_keeper
            .get_all_balances::<DB, AK, InitContext<'_, DB, SK>>(
                ctx,
                bonded_pool.base_account.address.clone(),
            );
        if bonded_balance
            .clone()
            .into_iter()
            .any(|e| e.amount.is_zero())
        {
            self.auth_keeper.set_module_account(ctx, bonded_pool);
        }
        // if balance is different from bonded coins panic because genesis is most likely malformed
        if bonded_balance != bonded_coins {
            return Err(AppError::Custom(format!(
                "bonded pool balance is different from bonded coins: {:?} <-> {:?}",
                bonded_balance, bonded_coins
            ))
            .into());
        }

        let not_bonded_pool = self
            .get_not_bonded_pool(ctx)
            .ok_or(AppError::Custom(format!(
                "{} module account has not been set",
                NOT_BONDED_POOL_NAME
            )))?;
        let not_bonded_balance = self
            .bank_keeper
            .get_all_balances::<DB, AK, InitContext<'_, DB, SK>>(
                ctx,
                not_bonded_pool.base_account.address.clone(),
            );
        if not_bonded_balance
            .clone()
            .into_iter()
            .any(|e| e.amount.is_zero())
        {
            self.auth_keeper.set_module_account(ctx, not_bonded_pool);
        }
        // if balance is different from non bonded coins panic because genesis is most likely malformed
        if not_bonded_balance != not_bonded_coins {
            return Err(AppError::Custom(format!(
                "not bonded pool balance is different from not bonded coins: {:?} <-> {:?}",
                bonded_balance, bonded_coins
            ))
            .into());
        }

        let mut res = vec![];
        // don't need to run Tendermint updates if we exported
        if genesis.exported {
            for last_validator in genesis.last_validator_powers {
                self.set_last_validator_power(ctx, &last_validator)?;
                let validator =
                    self.get_validator(ctx, last_validator.address.to_string().as_bytes())?;
                let mut update = validator.abci_validator_update(self.power_reduction(ctx));
                update.power = last_validator.power;
                res.push(update);
            }
        } else {
            self.apply_and_return_validator_set_dates(ctx)?;
        }
        Ok(res)
    }

    pub fn set_pool<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        pool: Pool,
    ) -> anyhow::Result<()> {
        let store = ctx.kv_store_mut(&self.store_key);
        let mut pool_store = store.prefix_store_mut(POOL_KEY);
        let pool = serde_json::to_vec(&pool)?;
        pool_store.set(pool.clone(), pool);
        Ok(())
    }

    /// BlockValidatorUpdates calculates the ValidatorUpdates for the current block
    /// Called in each EndBlock
    pub fn block_validator_updates<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
    ) -> anyhow::Result<Vec<ValidatorUpdate>> {
        // Calculate validator set changes.

        // NOTE: ApplyAndReturnValidatorSetUpdates has to come before
        // UnbondAllMatureValidatorQueue.
        // This fixes a bug when the unbonding period is instant (is the case in
        // some of the tests). The test expected the validator to be completely
        // unbonded after the Endblocker (go from Bonded -> Unbonding during
        // ApplyAndReturnValidatorSetUpdates and then Unbonding -> Unbonded during
        // UnbondAllMatureValidatorQueue).
        let validator_updates = self.apply_and_return_validator_set_dates(ctx)?;

        // unbond all mature validators from the unbonding queue
        self.unbond_all_mature_validators(ctx)?;

        // Remove all mature unbonding delegations from the ubd queue.
        let mature_unbonds = self.dequeue_all_mature_ubd_queue(ctx, Utc::now())?;
        for dv_pair in mature_unbonds {
            let val_addr = dv_pair.val_addr;
            let val_addr_str = val_addr.to_string();
            let del_addr = dv_pair.del_addr;
            let del_addr_str = del_addr.to_string();
            let balances = self.complete_unbonding(ctx, val_addr, del_addr)?;

            ctx.push_event(Event {
                r#type: EVENT_TYPE_COMPLETE_UNBONDING.to_string(),
                attributes: vec![
                    EventAttribute {
                        key: ATTRIBUTE_KEY_AMOUNT.as_bytes().into(),
                        value: serde_json::to_vec(&balances)?.into(),
                        index: false,
                    },
                    EventAttribute {
                        key: ATTRIBUTE_KEY_VALIDATOR.as_bytes().into(),
                        value: val_addr_str.as_bytes().to_vec().into(),
                        index: false,
                    },
                    EventAttribute {
                        key: ATTRIBUTE_KEY_DELEGATOR.as_bytes().into(),
                        value: del_addr_str.as_bytes().to_vec().into(),
                        index: false,
                    },
                ],
            });
        }
        // Remove all mature redelegations from the red queue.
        let mature_redelegations = self.dequeue_all_mature_redelegation_queue(ctx, Utc::now())?;
        for dvv_triplet in mature_redelegations {
            let val_src_addr = dvv_triplet.val_src_addr;
            let val_src_addr_str = val_src_addr.to_string();
            let val_dst_addr = dvv_triplet.val_dst_addr;
            let val_dst_addr_str = val_dst_addr.to_string();
            let del_addr = dvv_triplet.del_addr;
            let del_addr_str = del_addr.to_string();
            let balances = self.complete_redelegation(ctx, del_addr, val_src_addr, val_dst_addr)?;
            ctx.push_event(Event {
                r#type: EVENT_TYPE_COMPLETE_REDELEGATION.to_string(),
                attributes: vec![
                    EventAttribute {
                        key: ATTRIBUTE_KEY_AMOUNT.as_bytes().into(),
                        value: serde_json::to_vec(&balances)?.into(),
                        index: false,
                    },
                    EventAttribute {
                        key: ATTRIBUTE_KEY_DELEGATOR.as_bytes().into(),
                        value: del_addr_str.as_bytes().to_vec().into(),
                        index: false,
                    },
                    EventAttribute {
                        key: ATTRIBUTE_KEY_VALIDATOR.as_bytes().into(),
                        value: val_src_addr_str.as_bytes().to_vec().into(),
                        index: false,
                    },
                    EventAttribute {
                        key: ATTRIBUTE_KEY_VALIDATOR.as_bytes().into(),
                        value: val_dst_addr_str.as_bytes().to_vec().into(),
                        index: false,
                    },
                ],
            });
        }
        Ok(validator_updates)
    }

    /// ApplyAndReturnValidatorSetUpdates applies and return accumulated updates to the bonded validator set. Also,
    /// * Updates the active valset as keyed by LastValidatorPowerKey.
    /// * Updates the total power as keyed by LastTotalPowerKey.
    /// * Updates validator status' according to updated powers.
    /// * Updates the fee pool bonded vs not-bonded tokens.
    /// * Updates relevant indices.
    /// It gets called once after genesis, another time maybe after genesis transactions,
    /// then once at every EndBlock.
    ///
    /// CONTRACT: Only validators with non-zero power or zero-power that were bonded
    /// at the previous block height or were removed from the validator set entirely
    /// are returned to Tendermint.
    pub fn apply_and_return_validator_set_dates<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
    ) -> anyhow::Result<Vec<ValidatorUpdate>> {
        let params = self.staking_params_keeper.get(&ctx.multi_store())?;
        let max_validators = params.max_validators;
        let power_reduction = self.power_reduction(ctx);
        let mut total_power = 0;
        let mut amt_from_bonded_to_not_bonded = Uint256::zero();
        let amt_from_not_bonded_to_bonded = Uint256::zero();

        let mut last = self.get_last_validators_by_addr(ctx)?;
        let validators_map = self.get_validators_power_store_vals_map(ctx)?;

        let mut updates = vec![];

        for (_k, val_addr) in validators_map.iter().take(max_validators as usize) {
            // everything that is iterated in this loop is becoming or already a
            // part of the bonded validator set
            let mut validator: Validator =
                self.get_validator(ctx, val_addr.to_string().as_bytes())?;

            if validator.jailed {
                return Err(AppError::Custom(
                    "should never retrieve a jailed validator from the power store".to_string(),
                )
                .into());
            }
            // if we get to a zero-power validator (which we don't bond),
            // there are no more possible bonded validators
            if validator.tokens_to_consensus_power(self.power_reduction(ctx)) == 0 {
                break;
            }

            // apply the appropriate state change if necessary
            match validator.status {
                BondStatus::Unbonded => {
                    self.unbonded_to_bonded(ctx, &mut validator)?;
                    amt_from_bonded_to_not_bonded =
                        amt_from_not_bonded_to_bonded + validator.tokens.amount;
                }
                BondStatus::Unbonding => {
                    self.unbonding_to_bonded(ctx, &mut validator)?;
                    amt_from_bonded_to_not_bonded =
                        amt_from_not_bonded_to_bonded + validator.tokens.amount;
                }
                BondStatus::Bonded => {}
            }

            // fetch the old power bytes
            let val_addr_str = val_addr.to_string();
            let old_power_bytes = last.get(&val_addr_str);
            let new_power = validator.consensus_power(power_reduction);
            let new_power_bytes = new_power.to_be_bytes();
            // update the validator set if power has changed
            if old_power_bytes.is_none()
                || old_power_bytes.map(|v| v.as_slice()) != Some(&new_power_bytes)
            {
                updates.push(validator.abci_validator_update(power_reduction));

                self.set_last_validator_power(
                    ctx,
                    &LastValidatorPower {
                        address: val_addr.clone(),
                        power: new_power,
                    },
                )?;
            }

            last.remove(&val_addr_str);

            total_power += new_power;
        }

        let no_longer_bonded = sort_no_longer_bonded(&last)?;

        for val_addr_bytes in no_longer_bonded {
            let mut validator = self.get_validator(ctx, val_addr_bytes.as_bytes())?;
            self.bonded_to_unbonding(ctx, &mut validator)?;
            amt_from_bonded_to_not_bonded = amt_from_not_bonded_to_bonded + validator.tokens.amount;
            self.delete_last_validator_power(ctx, &validator.operator_address)?;
            updates.push(validator.abci_validator_update_zero());
        }

        // Update the pools based on the recent updates in the validator set:
        // - The tokens from the non-bonded candidates that enter the new validator set need to be transferred
        // to the Bonded pool.
        // - The tokens from the bonded validators that are being kicked out from the validator set
        // need to be transferred to the NotBonded pool.
        // Compare and subtract the respective amounts to only perform one transfer.
        // This is done in order to avoid doing multiple updates inside each iterator/loop.
        match amt_from_bonded_to_not_bonded.cmp(&amt_from_not_bonded_to_bonded) {
            Ordering::Greater => {
                self.not_bonded_tokens_to_bonded(
                    ctx,
                    amt_from_bonded_to_not_bonded - amt_from_not_bonded_to_bonded,
                )?;
            }
            Ordering::Less => {
                self.bonded_tokens_to_not_bonded(
                    ctx,
                    amt_from_bonded_to_not_bonded - amt_from_not_bonded_to_bonded,
                )?;
            }
            Ordering::Equal => {}
        }

        // set total power on lookup index if there are any updates
        if !updates.is_empty() {
            self.set_last_total_power(ctx, Uint256::from_u128(total_power as u128));
        }
        Ok(updates)
    }

    pub fn power_reduction<DB: Database, CTX: QueryableContext<DB, SK>>(&self, _ctx: &CTX) -> i64 {
        // TODO: sdk constant in cosmos
        1_000_000
    }

    pub fn not_bonded_tokens_to_bonded<DB: Database, CTX: TransactionalContext<DB, SK>>(
        &self,
        ctx: &mut CTX,
        amount: Uint256,
    ) -> anyhow::Result<()> {
        let params = self.staking_params_keeper.get(&ctx.multi_store())?;
        let coins = SendCoins::new(vec![Coin {
            denom: params.bond_denom,
            amount,
        }])?;
        Ok(self
            .bank_keeper
            .send_coins_from_module_to_module::<DB, AK, CTX>(
                ctx,
                NOT_BONDED_POOL_NAME.into(),
                BONDED_POOL_NAME.into(),
                coins,
            )?)
    }
}

/// given a map of remaining validators to previous bonded power
/// returns the list of validators to be unbonded, sorted by operator address
fn sort_no_longer_bonded(last: &HashMap<String, Vec<u8>>) -> anyhow::Result<Vec<String>> {
    let mut no_longer_bonded = last.iter().map(|(k, _v)| k.clone()).collect::<Vec<_>>();
    // sorted by address - order doesn't matter
    no_longer_bonded.sort();
    Ok(no_longer_bonded)
}
