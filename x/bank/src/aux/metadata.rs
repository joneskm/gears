use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gears::{config::ConfigDirectory, types::tx::metadata::Metadata};

#[derive(Debug, Clone)]
pub struct CoinsMetaGenesisCmd {
    pub home: PathBuf,
    pub metadata: String,
    pub dedup_input: bool,
    pub fail_on_dup: bool,
    pub overwrite_same: bool,
}

pub fn add_coins_meta_to_genesis(
    home: impl AsRef<Path>,
    metadata: impl IntoIterator<Item = Metadata>,
    dedup_input: bool,
    fail_on_dup: bool,
    overwrite_same: bool,
) -> anyhow::Result<()> {
    let metadata = {
        let mut metadata = metadata.into_iter().collect::<Vec<_>>();
        let pre_dup_len = metadata.len();

        metadata.dedup();

        if !dedup_input && ((pre_dup_len != metadata.len()) == true) {
            Err(anyhow::anyhow!("Found duplicates in new list"))?
        }

        metadata
    };

    let genesis_path = ConfigDirectory::GenesisFile.path_from_hone(&home);

    let mut genesis = serde_json::from_slice::<serde_json::Value>(&std::fs::read(&genesis_path)?)?;

    let value = genesis
        .pointer_mut("/app_state/bank/denom_metadata")
        .ok_or(anyhow::anyhow!(
            "`/app_state/bank/denom_metadata` not found. Check is genesis file is valid"
        ))?
        .take();

    let mut original_meta = serde_json::from_value::<Vec<Metadata>>(value)?
        .into_iter()
        .map(|this| (this.name.clone(), this))
        .collect::<HashMap<_, _>>();

    for meta in metadata {
        let dup = original_meta.get(&meta.name);

        match dup {
            Some(dup) => {
                if fail_on_dup {
                    Err(anyhow::anyhow!("Duplicate meta: {}", dup.name))?
                }

                if !overwrite_same && ((dup == &meta) == true) {
                    Err(anyhow::anyhow!(
                        "Similar meta with name: {}\nNew: {:#?}\nOriginal: {:#?}",
                        dup.name,
                        meta,
                        dup
                    ))?
                } else {
                    original_meta.insert(meta.name.clone(), meta);
                }
            }
            None => {
                original_meta.insert(meta.name.clone(), meta);
            }
        }
    }

    *genesis
        .pointer_mut("/app_state/bank/denom_metadata")
        .expect("we checked that this exists") = serde_json::to_value(
        original_meta
            .into_iter()
            .map(|(_, this)| this)
            .collect::<Vec<_>>(),
    )
    .expect("serde encoding");

    std::fs::write(
        genesis_path,
        serde_json::to_string_pretty(&genesis).expect("serde encoding"),
    )?;

    Ok(())
}