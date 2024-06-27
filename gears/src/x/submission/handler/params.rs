use std::marker::PhantomData;

use database::Database;
use kv_store::StoreKey;

use crate::{
    application::keepers::params::ParamsKeeper,
    context::{InfallibleContextMut, TransactionalContext},
    params::{gas::subspace_mut, infallible_subspace_mut, ParamsSubspaceKey},
    x::submission::{error::SubmissionError, param::ParamChange},
};

use super::{SubmissionCheckHandler, SubmissionHandler};

#[derive(Debug, Default)]
pub struct ParamChangeSubmissionHandler<PSK>(PhantomData<PSK>);

impl<PSK: ParamsSubspaceKey> SubmissionHandler<PSK, ParamChange<PSK>>
    for ParamChangeSubmissionHandler<PSK>
{
    fn handle<
        CTX: TransactionalContext<DB, SK>,
        PK: ParamsKeeper<PSK>,
        DB: Database,
        SK: StoreKey,
    >(
        &self,
        proposal: ParamChange<PSK>,
        ctx: &mut CTX,
        keeper: &mut PK,
    ) -> Result<(), SubmissionError> {
        if !self.submission_check::<PK>(&proposal) {
            Err(anyhow::anyhow!(
                "Can't handle this proposal: no such keys in subspace"
            ))?
        }

        let mut store = subspace_mut(ctx, keeper.psk());

        store.raw_key_set(proposal.key, proposal.value)?;

        Ok(())
    }

    fn infallible_gas_handle<
        CTX: InfallibleContextMut<DB, SK>,
        PK: ParamsKeeper<PSK>,
        DB: Database,
        SK: StoreKey,
    >(
        &self,
        proposal: ParamChange<PSK>,
        ctx: &mut CTX,
        keeper: &mut PK,
    ) -> anyhow::Result<()> {
        if !self.submission_check::<PK>(&proposal) {
            Err(anyhow::anyhow!(
                "Can't handle this proposal: no such keys in subspace"
            ))?
        }

        let mut store = infallible_subspace_mut(ctx, keeper.psk());

        store.raw_key_set(proposal.key, proposal.value);

        Ok(())
    }
}
