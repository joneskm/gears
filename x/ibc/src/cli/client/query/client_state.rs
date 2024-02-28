use clap::Args;
use gears::{client::query::run_query, types::context::query_context::QueryContext};
use prost::Message;
use proto_messages::cosmos::ibc::{
    query::{QueryClientStateResponse, RawQueryClientStateResponse},
    types::core::client::context::types::proto::v1::QueryClientStateRequest,
};
use tendermint::informal::block::Height;

#[derive(Args, Debug, Clone)]
pub struct CliClientParams {
    client_id: String,
}

#[allow(dead_code)]
pub(super) fn query_command_handler<DB, SK>(
    _ctx: &QueryContext<'_, DB, SK>,
    args: CliClientParams,
    node: &str,
    height: Option<Height>,
) -> anyhow::Result<String> {
    let query = QueryClientStateRequest {
        client_id: args.client_id,
    };

    let result = run_query::<QueryClientStateResponse, RawQueryClientStateResponse>(
        query.encode_to_vec(),
        "/ibc.core.client.v1.Query/UpgradedClientState".to_owned(),
        node,
        height,
    )?;

    let result = serde_json::to_string_pretty(&result)?;

    Ok(result)
}