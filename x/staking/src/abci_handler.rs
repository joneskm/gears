use crate::{
    BankKeeper, GenesisState, Keeper, KeeperHooks, Message, QueryDelegationRequest,
    QueryRedelegationRequest, QueryValidatorRequest,
};
use gears::{
    context::{block::BlockContext, init::InitContext, query::QueryContext, tx::TxContext},
    core::{errors::Error, Protobuf},
    error::AppError,
    params::ParamsSubspaceKey,
    store::{database::Database, StoreKey},
    tendermint::types::{
        proto::validator::ValidatorUpdate,
        request::{
            begin_block::RequestBeginBlock, end_block::RequestEndBlock, query::RequestQuery,
        },
    },
    x::{keepers::auth::AuthKeeper, module::Module},
};

#[derive(Debug, Clone)]
pub struct ABCIHandler<
    SK: StoreKey,
    PSK: ParamsSubspaceKey,
    AK: AuthKeeper<SK, M>,
    BK: BankKeeper<SK, M>,
    KH: KeeperHooks<SK, AK, M>,
    M: Module,
> {
    keeper: Keeper<SK, PSK, AK, BK, KH, M>,
}

impl<
        SK: StoreKey,
        PSK: ParamsSubspaceKey,
        AK: AuthKeeper<SK, M>,
        BK: BankKeeper<SK, M>,
        KH: KeeperHooks<SK, AK, M>,
        M: Module,
    > ABCIHandler<SK, PSK, AK, BK, KH, M>
{
    pub fn new(keeper: Keeper<SK, PSK, AK, BK, KH, M>) -> Self {
        ABCIHandler { keeper }
    }

    pub fn tx<DB: Database + Sync + Send>(
        &self,
        ctx: &mut TxContext<'_, DB, SK>,
        msg: &Message,
    ) -> Result<(), AppError> {
        match msg {
            Message::CreateValidator(msg) => self.keeper.create_validator(ctx, msg),
            Message::Delegate(msg) => self.keeper.delegate_cmd_handler(ctx, msg),
            Message::Redelegate(msg) => self.keeper.redelegate_cmd_handler(ctx, msg),
        }
    }

    pub fn genesis<DB: Database>(&self, ctx: &mut InitContext<'_, DB, SK>, genesis: GenesisState) {
        self.keeper.init_genesis(ctx, genesis);
    }

    pub fn query<DB: Database + Send + Sync>(
        &self,
        ctx: &QueryContext<DB, SK>,
        query: RequestQuery,
    ) -> Result<prost::bytes::Bytes, AppError> {
        match query.path.as_str() {
            "/cosmos.staking.v1beta1.Query/Validator" => {
                let req = QueryValidatorRequest::decode(query.data)
                    .map_err(|e| Error::DecodeProtobuf(e.to_string()))?;

                Ok(self.keeper.query_validator(ctx, req).encode_vec().into())
            }
            "/cosmos.staking.v1beta1.Query/Delegation" => {
                let req = QueryDelegationRequest::decode(query.data)
                    .map_err(|e| Error::DecodeProtobuf(e.to_string()))?;

                Ok(self.keeper.query_delegation(ctx, req).encode_vec().into())
            }
            "/cosmos.staking.v1beta1.Query/Redelegation" => {
                let req = QueryRedelegationRequest::decode(query.data)
                    .map_err(|e| Error::DecodeProtobuf(e.to_string()))?;

                Ok(self
                    .keeper
                    .query_redelegations(ctx, req)
                    .encode_vec()
                    .into())
            }
            _ => Err(AppError::InvalidRequest("query path not found".into())),
        }
    }

    pub fn begin_block<DB: Database>(
        &self,
        ctx: &mut BlockContext<'_, DB, SK>,
        _request: RequestBeginBlock,
    ) {
        self.keeper.track_historical_info(ctx);
        // TODO
        // defer telemetry.ModuleMeasureSince(types.ModuleName, time.Now(), telemetry.MetricKeyBeginBlocker)
    }

    pub fn end_block<DB: Database>(
        &self,
        ctx: &mut BlockContext<'_, DB, SK>,
        _request: RequestEndBlock,
    ) -> Vec<ValidatorUpdate> {
        self.keeper.block_validator_updates(ctx)
        // TODO
        // defer telemetry.ModuleMeasureSince(types.ModuleName, time.Now(), telemetry.MetricKeyEndBlocker)
    }
}
