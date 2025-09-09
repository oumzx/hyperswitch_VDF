// utils.rs est dans crates/router/tests/utils.rs (un niveau au-dessus)
#[path = "../utils.rs"]
mod utils;
use utils::AppClient;

use serial_test::serial;
use std::ops::Deref;

const CONNECTOR: &str = "wave";

// Optionnel: récupère une API key admin depuis l'env, sinon "test_admin"
fn admin_key() -> String {
    std::env::var("ADMIN_API_KEY").unwrap_or_else(|_| "test_admin".to_string())
}

#[actix_rt::test]
#[serial]
async fn wave_smoke_health() {
    utils::setup().await;
    let app = utils::mk_service().await;

    // Client invité: pas d’auth
    let guest = AppClient::guest();
    let health = guest.health(&app).await;
    assert!(
        health.to_ascii_lowercase().contains("health is good")
            || health.to_ascii_lowercase().contains("OK"),
        "health endpoint should be health is good, got: {}",
        health
    );
}

#[actix_rt::test]
#[serial]
async fn wave_attach_connector_and_check() {
    utils::setup().await;
    let app = utils::mk_service().await;

    // Crée un admin et un merchant
    let admin = AppClient::guest().admin(&admin_key());
    let merchant_resp: utils::HCons<utils::MerchantId, utils::HNil> =
        admin.create_merchant_account(&app, None).await;
    let merchant_id = merchant_resp.head.deref().clone();

    // Lis la clé du connecteur Wave (fichier/env); à adapter à ton setup
    // Par exemple: export WAVE_API_KEY="sk_test_..."
    let wave_api_key = std::env::var("WAVE_API_KEY").unwrap_or_else(|_| "dummy".to_string());

    // Rattache le connecteur Wave
    let _connector_created: serde_json::Value = admin
        .create_connector(&app, &merchant_id, CONNECTOR, &wave_api_key)
        .await;

    // Ici tu peux poursuivre avec la création d’un payment si tu as déjà
    // implémenté l’endpoint Wave côté routeur. Sinon,
    // Temporaire, juste pour inspecter
    let raw: serde_json::Value = admin.create_merchant_account(&app, None).await;
    eprintln!("merchant create raw = {}", raw);

}

use masking::Secret;
use router::types::{self, storage::enums};
use common_utils::{pii::Email, types::MinorUnit};
use common_enums::Currency;
use std::str::FromStr;

use crate::{
    connector_auth,
    utils::{ConnectorActions, Connector, construct_connector_data_old, to_connector_auth_type, get_connector_transaction_id, PaymentAuthorizeType, PaymentRefundType},
};

struct Wave;
impl ConnectorActions for Wave {}
impl Connector for Wave {
    fn get_data(&self) -> types::api::ConnectorData {
        use router::connector::Wave;
        construct_connector_data_old(
            Box::new(&Wave),
            types::Connector::Wave,
            types::api::GetToken::Connector,
            None,
        )
    }

    fn get_auth_token(&self) -> types::ConnectorAuthType {
        to_connector_auth_type(
            connector_auth::ConnectorAuthentication::new()
                .wave
                .expect("Missing Wave connector authentication configuration")
                .into(),
        )
    }

    fn get_name(&self) -> String {
        "wave".to_string()
    }
}

fn get_payment_authorize_data() -> Option<types::PaymentsAuthorizeData> {
    Some(types::PaymentsAuthorizeData {
        payment_method_data: types::domain::PaymentMethodData::Crypto(types::domain::CryptoData {
            pay_currency: None,
            network: None,
        }),
        currency: Currency::XOF,
        amount: 1000, // 10 XOF
        minor_amount: MinorUnit::new(1000),
        email: Some(Email::from_str("test@wave.com").unwrap()),
        customer_name: Some(Secret::new("John Doe".to_string())),
        router_return_url: Some("https://example.com/return".to_string()),
        ..PaymentAuthorizeType::default().0
    })
}

fn get_payment_authorize_data_usd() -> Option<types::PaymentsAuthorizeData> {
    Some(types::PaymentsAuthorizeData {
        payment_method_data: types::domain::PaymentMethodData::Crypto(types::domain::CryptoData {
            pay_currency: None,
            network: None,
        }),
        currency: Currency::USD,
        amount: 100, // $1.00
        minor_amount: MinorUnit::new(100),
        email: Some(Email::from_str("test@wave.com").unwrap()),
        customer_name: Some(Secret::new("Jane Doe".to_string())),
        router_return_url: Some("https://example.com/return".to_string()),
        ..PaymentAuthorizeType::default().0
    })
}

// Payment Authorization Tests

#[actix_web::test]
async fn should_only_authorize_payment() {
    let response = Wave {}
        .authorize_payment(get_payment_authorize_data(), None)
        .await
        .unwrap();
    assert_eq!(response.status, enums::AttemptStatus::Pending);
    // Wave payments should have redirection data for checkout sessions
    let resp = response.response.ok().unwrap();
    let endpoint = match resp {
        types::PaymentsResponseData::TransactionResponse {
            redirection_data, ..
        } => Some(redirection_data),
        _ => None,
    };
    assert!(endpoint.is_some());
}

#[actix_web::test]
async fn should_authorize_payment_with_usd() {
    let response = Wave {}
        .authorize_payment(get_payment_authorize_data_usd(), None)
        .await
        .unwrap();
    assert_eq!(response.status, enums::AttemptStatus::Pending);
    let resp = response.response.ok().unwrap();
    let endpoint = match resp {
        types::PaymentsResponseData::TransactionResponse {
            redirection_data, ..
        } => Some(redirection_data),
        _ => None,
    };
    assert!(endpoint.is_some());
}

#[actix_web::test]
async fn should_sync_authorized_payment() {
    let connector = Wave {};
    let authorize_response = connector
        .authorize_payment(get_payment_authorize_data(), None)
        .await
        .unwrap();
    let txn_id = get_connector_transaction_id(authorize_response.response);
    let response = connector
        .psync_retry_till_status_matches(
            enums::AttemptStatus::Pending,
            Some(types::PaymentsSyncData {
                connector_transaction_id: types::ResponseId::ConnectorTransactionId(
                    txn_id.unwrap(),
                ),
                ..Default::default()
            }),
            None,
        )
        .await
        .unwrap();
    // Wave payment status should be pending, completed, failed, or cancelled
    assert!(matches!(
        response.status,
        enums::AttemptStatus::Pending
            | enums::AttemptStatus::Charged
            | enums::AttemptStatus::Failure
            | enums::AttemptStatus::Voided
    ));
}

#[actix_web::test]
async fn should_void_authorized_payment() {
    let connector = Wave {};
    let response = connector
        .authorize_and_void_payment(
            get_payment_authorize_data(),
            Some(types::PaymentsCancelData {
                connector_transaction_id: "".to_string(),
                cancellation_reason: Some("requested_by_customer".to_string()),
                ..Default::default()
            }),
            None,
        )
        .await;
    assert_eq!(response.unwrap().status, enums::AttemptStatus::Voided);
}

// Refund Tests

#[actix_web::test]
async fn should_refund_succeeded_payment() {
    let connector = Wave {};
    // For Wave, we need to authorize a payment first
    let authorize_response = connector
        .authorize_payment(get_payment_authorize_data(), None)
        .await
        .unwrap();
    
    let txn_id = get_connector_transaction_id(authorize_response.response).unwrap();
    
    // Attempt to refund
    let response = connector
        .refund_payment(
            txn_id,
            Some(types::RefundsData {
                refund_amount: 500, // Partial refund
                reason: Some("customer_request".to_string()),
                ..PaymentRefundType::default().0
            }),
            None,
        )
        .await
        .unwrap();
    
    assert!(matches!(
        response.response.unwrap().refund_status,
        enums::RefundStatus::Pending | enums::RefundStatus::Success
    ));
}

#[actix_web::test]
async fn should_sync_refund() {
    let connector = Wave {};
    // First authorize a payment
    let authorize_response = connector
        .authorize_payment(get_payment_authorize_data(), None)
        .await
        .unwrap();
    
    let txn_id = get_connector_transaction_id(authorize_response.response).unwrap();
    
    // Create a refund
    let refund_response = connector
        .refund_payment(
            txn_id,
            Some(types::RefundsData {
                refund_amount: 500,
                reason: Some("customer_request".to_string()),
                ..PaymentRefundType::default().0
            }),
            None,
        )
        .await
        .unwrap();
    
    let refund_id = refund_response.response.unwrap().connector_refund_id;
    
    // Sync the refund
    let response = connector
        .rsync_retry_till_status_matches(
            enums::RefundStatus::Success,
            refund_id,
            None,
            None,
        )
        .await
        .unwrap();
    
    assert!(matches!(
        response.response.unwrap().refund_status,
        enums::RefundStatus::Pending
            | enums::RefundStatus::Success
            | enums::RefundStatus::Failure
    ));
}

// Error Handling Tests

#[actix_web::test]
async fn should_fail_payment_for_invalid_currency() {
    let response = Wave {}
        .authorize_payment(
            Some(types::PaymentsAuthorizeData {
                currency: Currency::JPY, // Unsupported currency
                minor_amount: MinorUnit::new(1000),
                ..get_payment_authorize_data().unwrap()
            }),
            None,
        )
        .await;
    
    // Should return error for unsupported currency
    assert!(response.is_err());
}

#[actix_web::test]
async fn should_fail_payment_for_invalid_amount() {
    let response = Wave {}
        .authorize_payment(
            Some(types::PaymentsAuthorizeData {
                amount: 0, // Invalid amount
                minor_amount: MinorUnit::new(0),
                ..get_payment_authorize_data().unwrap()
            }),
            None,
        )
        .await;
    
    assert!(response.is_err());
}
