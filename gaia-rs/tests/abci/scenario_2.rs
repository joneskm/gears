use std::path::Path;

use gears::{
    tendermint::types::{proto::crypto::PublicKey, time::timestamp::Timestamp},
    types::uint::Uint256,
    utils::node::generate_tx,
};
use staking::{CommissionRates, CreateValidator, Description, EditDescription};

use crate::{setup_mock_node, USER_0, USER_1};

#[test]
/// This scenario has a richer genesis file, with more staking fields.
fn scenario_2() {
    let genesis_path = Path::new("./tests/abci/assets/scenario_2_genesis.json");
    let (mut node, _) = setup_mock_node(Some(genesis_path));
    let user_0 = crate::user(5, USER_0);
    let user_1 = crate::user(6, USER_1);

    let app_hash = node.step(vec![], Timestamp::UNIX_EPOCH).app_hash;
    assert_eq!(
        hex::encode(app_hash),
        "84fe8533042f839ac30c47c06b2488561c8fd06baea2e6a3e3ed89548d575ebb"
    );

    //----------------------------------------
    // Create a validator

    let consensus_pub_key = serde_json::from_str::<PublicKey>(
        r#"{
    "type": "tendermint/PubKeyEd25519",
    "value": "NJWo4rSXCswNmK0Bttxzb8/1ioFNkRVi6Fio2KzAlCo="
    }"#,
    )
    .expect("hardcoded is valid");

    let msg =
        gaia_rs::message::Message::Staking(staking::Message::CreateValidator(CreateValidator {
            description: Description {
                moniker: "test".to_string(),
                identity: "".to_string(),
                website: "".to_string(),
                details: "".to_string(),
                security_contact: "".to_string(),
            },
            commission: CommissionRates::new(
                "0.1".parse().expect("hardcoded is valid"),
                "1".parse().expect("hardcoded is valid"),
                "0.1".parse().expect("hardcoded is valid"),
            )
            .expect("hardcoded is valid"),
            min_self_delegation: Uint256::from(100u32),
            delegator_address: user_1.address(),
            validator_address: user_1.address().into(),
            pubkey: consensus_pub_key,
            value: "10000uatom".parse().expect("hardcoded is valid"),
        }));

    let txs = generate_tx(vec1::vec1![msg], 0, &user_1, node.chain_id().clone());

    let step_response = node.step(
        vec![txs],
        Timestamp::try_new(0, 0).expect("hardcoded is valid"),
    );
    assert_eq!(step_response.tx_responses[0].code, 0);
    assert_eq!(
        hex::encode(step_response.app_hash),
        "ca9fb5ffb66dec17738673048038e73e24168e61c3167b62c54ab90cd677cb32"
    );

    //----------------------------------------
    // Edit a validator - successfully

    let msg = gaia_rs::message::Message::Staking(staking::Message::EditValidator(
        staking::EditValidator::new(
            EditDescription {
                moniker: Some("alice".to_string()),
                identity: Some("".to_string()),
                website: Some("".to_string()),
                security_contact: Some("".to_string()),
                details: Some("".to_string()),
            },
            Some("0.2".parse().expect("hardcoded is valid")),
            Some(Uint256::from(200u32)),
            user_1.address().into(),
        ),
    ));

    let txs = generate_tx(vec1::vec1![msg], 1, &user_1, node.chain_id().clone());

    let step_response = node.step(
        vec![txs],
        Timestamp::try_new(60 * 60 * 24, 0).expect("hardcoded is valid"),
    );
    assert_eq!(step_response.tx_responses[0].code, 0);
    assert_eq!(
        hex::encode(step_response.app_hash),
        "b67270d64726252330733d3f6955d7cd89230e342044798829c41a36af40e03a"
    );

    //----------------------------------------
    // Delegate to a validator

    let msg =
        gaia_rs::message::Message::Staking(staking::Message::Delegate(staking::DelegateMsg {
            validator_address: user_0.address().into(),
            amount: "1000uatom".parse().expect("hardcoded is valid"),
            delegator_address: user_1.address(),
        }));

    let txs = generate_tx(vec1::vec1![msg], 2, &user_1, node.chain_id().clone());

    let step_response = node.step(
        vec![txs],
        Timestamp::try_new(60 * 60 * 24, 0).expect("hardcoded is valid"),
    );
    assert_eq!(step_response.tx_responses[0].code, 0);

    assert_eq!(
        hex::encode(step_response.app_hash),
        "335a2ee38efbe2d58b0ddc18aab28bfdc25277aab44aeabbe7968a9d20afd39e"
    );

    //----------------------------------------
    // Redelegate from a validator to another validator

    let msg =
        gaia_rs::message::Message::Staking(staking::Message::Redelegate(staking::RedelegateMsg {
            delegator_address: user_1.address(),
            src_validator_address: user_0.address().into(),
            dst_validator_address: user_1.address().into(),
            amount: "500uatom".parse().expect("hardcoded is valid"),
        }));

    let txs = generate_tx(vec1::vec1![msg], 3, &user_1, node.chain_id().clone());

    let step_response = node.step(
        vec![txs],
        Timestamp::try_new(60 * 60 * 24, 0).expect("hardcoded is valid"),
    );
    assert_eq!(step_response.tx_responses[0].code, 0);

    assert_eq!(
        hex::encode(step_response.app_hash),
        "d2366f8070103134bd095fe4578441c64f065d6b5b62db5f5c05e774593c9dcb"
    );

    //----------------------------------------
    // Undelegate from a validator

    let msg =
        gaia_rs::message::Message::Staking(staking::Message::Undelegate(staking::UndelegateMsg {
            validator_address: user_0.address().into(),
            amount: "500uatom".parse().expect("hardcoded is valid"),
            delegator_address: user_1.address(),
        }));

    let txs = generate_tx(vec1::vec1![msg], 4, &user_1, node.chain_id().clone());

    let step_response = node.step(
        vec![txs],
        Timestamp::try_new(60 * 60 * 24, 0).expect("hardcoded is valid"),
    );
    assert_eq!(step_response.tx_responses[0].code, 0);

    assert_eq!(
        hex::encode(step_response.app_hash),
        "cd20c5db4bc2e575d82590df22770964128ae88a249ce176015268799cb9bd80"
    );

    //----------------------------------------
    // Jump forward in time

    let app_hash = node
        .step(vec![], Timestamp::try_new(60 * 60 * 24 * 30, 0).unwrap())
        .app_hash; // 30 days which is greater than the unbonding time
    assert_eq!(
        hex::encode(app_hash),
        "275fed647b2e14398e3f59abbae8b414c8c4783c08959246fda417b7d9f35ba9"
    );
}
