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

