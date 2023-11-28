use database::RocksDB;
use gears::types::context::context::Context;
use proto_messages::cosmos::tx::v1beta1::{
    screen::{Content, Indent, Screen},
    signer::SignerInfo,
};
use store::StoreKey;

use crate::signing::renderer::value_renderer::{
    DefaultPrimitiveRenderer, PrimitiveValueRenderer, ValueRenderer,
};

impl<DefaultValueRenderer, SK: StoreKey> ValueRenderer<DefaultValueRenderer, SK> for SignerInfo {
    fn format(
        &self,
        ctx: &Context<'_, '_, RocksDB, SK>,
    ) -> Result<Vec<Screen>, Box<dyn std::error::Error>> {
        let SignerInfo {
            public_key,
            mode_info,
            sequence,
        } = &self;

        let mut final_screens = Vec::<Screen>::new();
        if let Some(public_key) = public_key {
            final_screens.append(&mut ValueRenderer::<DefaultValueRenderer, SK>::format(
                public_key, ctx,
            )?)
        }

        if let Some(mode_info) = mode_info {
            final_screens.append(&mut ValueRenderer::<DefaultValueRenderer, SK>::format(
                mode_info, ctx,
            )?)
        }

        final_screens.push(Screen {
            title: "Sequence".to_string(),
            content: Content::new(DefaultPrimitiveRenderer::format(*sequence))?,
            indent: Some(Indent::new(2)?),
            expert: true,
        });

        Ok(final_screens)
    }
}

#[cfg(test)]
mod tests {
    use gears::types::context::context::Context;
    use proto_messages::cosmos::tx::v1beta1::{
        screen::{Content, Indent, Screen},
        signer::SignerInfo,
    };
    use proto_types::AccAddress;

    use crate::signing::renderer::{
        value_renderer::{DefaultValueRenderer, ValueRenderer},
        KeyMock, MockContext,
    };

    #[test]
    fn signer_info_formatting() -> anyhow::Result<()> {
        let info = SignerInfo {
            public_key: Some(serde_json::from_str(
                r#"{
                        "@type": "/cosmos.crypto.secp256k1.PubKey",
                        "key": "Auvdf+T963bciiBe9l15DNMOijdaXCUo6zqSOvH7TXlN"
                    }"#,
            )?),
            mode_info: None,
            sequence: 2,
        };

        let expected_screens = vec![
            Screen {
                title: "Public key".to_string(),
                content: Content::new("/cosmos.crypto.secp256k1.PubKey")?,
                indent: None,
                expert: true,
            },
            Screen {
                title: "Key".to_string(),
                content: Content::new(AccAddress::from_bech32(
                    "cosmos1ulav3hsenupswqfkw2y3sup5kgtqwnvqa8eyhs",
                )?)?,
                indent: Some(Indent::new(1)?),
                expert: true,
            },
            Screen {
                title: "Sequence".to_string(),
                content: Content::new(2.to_string())?,
                indent: Some(Indent::new(2)?),
                expert: true,
            },
        ];

        let mut ctx = MockContext;

        let context: Context<'_, '_, database::RocksDB, KeyMock> =
            Context::DynamicContext(&mut ctx);

        let actuals_screens =
            ValueRenderer::<DefaultValueRenderer, KeyMock>::format(&info, &context)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        assert_eq!(expected_screens, actuals_screens);

        Ok(())
    }
}