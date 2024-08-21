use std::path::PathBuf;

use gears::application::handlers::client::TxHandler;
use staking::{cli::tx::CreateValidatorCli, CreateValidator};

#[derive(Debug, Clone)]
pub struct GentxTxHandler {
    output_dir: PathBuf,
}

impl GentxTxHandler {
    pub fn new(output_dir: PathBuf) -> anyhow::Result<Self> {
        if output_dir.exists() && !output_dir.is_dir() {
            Err(anyhow::anyhow!("use directory"))?
        }

        std::fs::create_dir(&output_dir)?;

        Ok(Self { output_dir })
    }
}

impl TxHandler for GentxTxHandler {
    type Message = CreateValidator;

    type TxCommands = CreateValidatorCli;

    fn prepare_tx(
        &self,
        _client_tx_context: &gears::commands::client::tx::ClientTxContext,
        command: Self::TxCommands,
        from_address: gears::types::address::AccAddress,
    ) -> anyhow::Result<gears::types::tx::Messages<Self::Message>> {
        command.clone().try_into_cmd(from_address).map(Into::into)
    }

    fn handle_tx(
        &self,
        tx: gears::types::tx::Tx<Self::Message>,
        _node: url::Url,
    ) -> anyhow::Result<gears::application::handlers::client::TxExecutionResult> {
        let tx_str = serde_json::to_string_pretty(&tx)?;
        let output = self.output_dir.join("gentx.json");
        std::fs::write(&output, tx_str)?;

        Ok(gears::application::handlers::client::TxExecutionResult::File(output))
    }
}