use std::marker::PhantomData;

use anyhow::anyhow;
use gears::{
    application::keepers::params::ParamsKeeper,
    context::{init::InitContext, tx::TxContext, TransactionalContext},
    params::ParamsSubspaceKey,
    store::{database::Database, StoreKey},
    tendermint::types::proto::event::{Event, EventAttribute},
    types::{
        address::AccAddress,
        store::gas::{
            errors::GasStoreErrors,
            ext::GasResultExt,
            kv::{mutable::GasKVStoreMut, GasKVStore},
        },
    },
    x::{keepers::bank::BankKeeper, module::Module},
};

use crate::{
    errors::SERDE_JSON_CONVERSION,
    genesis::GovGenesisState,
    params::GovParamsKeeper,
    types::{
        deposit::Deposit,
        proposal::{Proposal, ProposalStatus},
    },
};

const PROPOSAL_ID_KEY: [u8; 1] = [0x03];
pub(crate) const KEY_PROPOSAL_PREFIX: [u8; 1] = [0x00];
pub(crate) const KEY_DEPOSIT_PREFIX: [u8; 1] = [0x10];
pub(crate) const KEY_VOTES_PREFIX: [u8; 1] = [0x20];

#[allow(dead_code)]
pub struct GovKeeper<SK: StoreKey, PSK: ParamsSubspaceKey, BM: Module, BK: BankKeeper<SK, BM>> {
    store_key: SK,
    gov_params_keeper: GovParamsKeeper<PSK>,
    gov_mod: BM,
    bank_keeper: BK,
    _bank_marker: PhantomData<BM>,
}

impl<SK: StoreKey, PSK: ParamsSubspaceKey, BM: Module, BK: BankKeeper<SK, BM>>
    GovKeeper<SK, PSK, BM, BK>
{
    pub fn new(store_key: SK, params_subspace_key: PSK, gov_mod: BM, bank_keeper: BK) -> Self {
        Self {
            store_key,
            gov_params_keeper: GovParamsKeeper {
                params_subspace_key,
            },
            gov_mod,
            bank_keeper,
            _bank_marker: PhantomData,
        }
    }

    pub fn init_genesis<DB: Database>(
        &self,
        ctx: &mut InitContext<'_, DB, SK>,
        GovGenesisState {
            starting_proposal_id,
            deposits,
            votes,
            proposals,
            params,
        }: GovGenesisState,
    ) {
        {
            let mut store = ctx.kv_store_mut(&self.store_key);
            store.set(PROPOSAL_ID_KEY, starting_proposal_id.to_be_bytes())
        }
        self.gov_params_keeper.set(ctx, params);

        let total_deposits = {
            let mut store_mut = ctx.kv_store_mut(&self.store_key);

            let total_deposits = {
                let mut total_deposits = Vec::with_capacity(deposits.len());
                for deposit in deposits {
                    store_mut.set(
                        deposit.key(),
                        serde_json::to_vec(&deposit).expect(SERDE_JSON_CONVERSION),
                    ); // TODO:NOW IS THIS CORRECT SERIALIZATION?
                    total_deposits.push(deposit.amount);
                }

                total_deposits.into_iter().flatten().collect::<Vec<_>>()
            };

            for vote in votes {
                store_mut.set(
                    vote.key(),
                    serde_json::to_vec(&vote).expect(SERDE_JSON_CONVERSION),
                )
            }

            for proposal in proposals {
                match proposal.status {
                    ProposalStatus::DepositPeriod => {
                        store_mut.set(
                            proposal.inactive_queue_key(),
                            proposal.proposal_id.to_be_bytes(),
                        );
                    }
                    ProposalStatus::VotingPeriod => store_mut.set(
                        proposal.active_queue_key(),
                        proposal.proposal_id.to_be_bytes(),
                    ),
                    _ => (),
                }

                store_mut.set(
                    proposal.key(),
                    serde_json::to_vec(&proposal).expect(SERDE_JSON_CONVERSION),
                );
            }

            total_deposits
        };

        let balance = self
            .bank_keeper
            .balance_all(ctx, &self.gov_mod.get_address())
            .unwrap_gas();
        /*
           Okay. I think that in our implementation there is no need to create account if it.

           So I should omit this lines...
           if balance.is_empty() || balance.iter().any(|this| this.amount.is_zero()) {
               https://github.com/cosmos/cosmos-sdk/blob/d3f09c222243bb3da3464969f0366330dcb977a8/x/gov/genesis.go#L47
           }
        */

        if !(balance == total_deposits) {
            panic!(
                "expected module account was {:?} but we got {:?}",
                balance, total_deposits
            )
        }
    }

    pub fn deposit_add<DB: Database>(
        &self,
        ctx: &mut TxContext<'_, DB, SK>,
        Deposit {
            proposal_id,
            depositor,
            amount,
        }: Deposit,
    ) -> anyhow::Result<bool> {
        let mut proposal = proposal_get(ctx.kv_store(&self.store_key), proposal_id)?
            .ok_or(anyhow!("unknown proposal {proposal_id}"))?;

        match proposal.status {
            ProposalStatus::DepositPeriod | ProposalStatus::VotingPeriod => Ok(()),
            _ => Err(anyhow!("inactive proposal {proposal_id}")),
        }?;

        self.bank_keeper.send_coins_from_account_to_module(
            ctx,
            depositor.clone(),
            &self.gov_mod,
            amount.clone(),
        )?;

        proposal.total_deposit = proposal.total_deposit.checked_add(amount.clone())?;
        proposal_set(ctx.kv_store_mut(&self.store_key), &proposal)?;

        let deposit_params = self.gov_params_keeper.try_get(ctx)?.deposit;
        let activated_voting_period = match proposal.status {
            ProposalStatus::DepositPeriod
                if proposal
                    .total_deposit
                    .is_all_gte(&deposit_params.min_deposit) =>
            {
                true
            }
            _ => false,
        };

        let deposit = match deposit_get(ctx.kv_store(&self.store_key), proposal_id, &depositor)? {
            Some(mut deposit) => {
                deposit.amount = deposit.amount.checked_add(amount)?;
                deposit
            }
            None => Deposit {
                proposal_id,
                depositor,
                amount,
            },
        };

        // TODO: ADD HOOK https://github.com/cosmos/cosmos-sdk/blob/d3f09c222243bb3da3464969f0366330dcb977a8/x/gov/keeper/deposit.go#L149

        ctx.push_event(Event::new(
            "proposal_deposit",
            vec![
                EventAttribute::new(
                    "amount".into(),
                    format!("{:?}", deposit.amount).into(),
                    false,
                ),
                EventAttribute::new(
                    "proposal_id".into(),
                    format!("{}", deposit.proposal_id).into(),
                    false,
                ),
            ],
        ));

        deposit_set(ctx.kv_store_mut(&self.store_key), &deposit)?;

        Ok(activated_voting_period)
    }
}

fn proposal_get<DB: Database>(
    store: GasKVStore<'_, DB>,
    proposal_id: u64,
) -> Result<Option<Proposal>, GasStoreErrors> {
    let key = [KEY_PROPOSAL_PREFIX.as_slice(), &proposal_id.to_be_bytes()].concat();

    let bytes = store.get(&key)?;
    match bytes {
        Some(var) => Ok(Some(
            serde_json::from_slice(&var).expect(SERDE_JSON_CONVERSION),
        )),
        None => Ok(None),
    }
}

fn proposal_set<DB: Database>(
    mut store: GasKVStoreMut<'_, DB>,
    proposal: &Proposal,
) -> Result<(), GasStoreErrors> {
    store.set(
        proposal.key(),
        serde_json::to_vec(proposal).expect(SERDE_JSON_CONVERSION),
    )
}

fn deposit_get<DB: Database>(
    store: GasKVStore<'_, DB>,
    proposal_id: u64,
    depositor: &AccAddress,
) -> Result<Option<Deposit>, GasStoreErrors> {
    let key = [
        KEY_DEPOSIT_PREFIX.as_slice(),
        &proposal_id.to_be_bytes(),
        &[depositor.len()],
        depositor.as_ref(),
    ]
    .concat();

    let bytes = store.get(&key)?;
    match bytes {
        Some(var) => Ok(Some(
            serde_json::from_slice(&var).expect(SERDE_JSON_CONVERSION),
        )),
        None => Ok(None),
    }
}

fn deposit_set<DB: Database>(
    mut store: GasKVStoreMut<'_, DB>,
    deposit: &Deposit,
) -> Result<(), GasStoreErrors> {
    store.set(
        deposit.key(),
        serde_json::to_vec(deposit).expect(SERDE_JSON_CONVERSION),
    )
}