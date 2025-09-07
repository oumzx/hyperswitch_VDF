// // crates/hyperswitch_connectors/src/connectors/wave/transformers.rs

// use serde::{Deserialize, Serialize};
// use masking::Secret;

// use hyperswitch_domain_models::{
//     router_data::ConnectorAuthType,
//     types::PaymentsAuthorizeRouterData,
// };
// use error_stack::{ResultExt, Report};

// use hyperswitch_interfaces::errors::ConnectorError;

// #[derive(Debug, Clone)]
// pub struct WaveAuthType {
//     pub api_key: Secret<String>,
// }

// impl TryFrom<&ConnectorAuthType> for WaveAuthType {
//     type Error = Report<ConnectorError>;
//     fn try_from(auth: &ConnectorAuthType) -> Result<Self, Self::Error> {
//         // Adapte selon la forme d’auth attendue pour Wave dans ton projet.
//         // Ici, on accepte une API Key simple stockée dans auth.api_key
//         let key = auth
//             .get_api_key() // si tu n'as pas cette méthode, remplace par le bon accès
//             .change_context(ConnectorError::FailedToObtainAuthType)?;
//         Ok(Self { api_key: Secret::new(key) })
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct CreateSessionRequest {
//     pub amount: i64,
//     pub currency: String,
//     pub reference: String,
//     pub return_url: Option<String>,
// }

// impl TryFrom<&PaymentsAuthorizeRouterData> for CreateSessionRequest {
//     type Error = Report<ConnectorError>;
//     fn try_from(req: &PaymentsAuthorizeRouterData) -> Result<Self, Self::Error> {
//         let amount = req.request.amount;
//         let currency = req.request.currency.to_string();
//         let reference = req.request.connector_request_reference_id.clone().unwrap_or_default();
//         let return_url = req.request.return_url.clone();
//         Ok(Self { amount, currency, reference, return_url })
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct CreateSessionResponse {
//     pub launch_url: Option<String>,
//     pub transaction_id: Option<String>,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct RefundRequest {
//     pub amount: i64,
// }

// impl<T> TryFrom<&hyperswitch_domain_models::types::RefundsRouterData<T>> for RefundRequest {
//     type Error = Report<ConnectorError>;
//     fn try_from(req: &hyperswitch_domain_models::types::RefundsRouterData<T>) -> Result<Self, Self::Error> {
//         Ok(Self { amount: req.request.amount })
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct RefundResp {
//     pub refund_id: Option<String>,
//     pub status: Option<String>,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct WaveErrorResponse {
//     pub error_code: Option<String>,
//     pub message: Option<String>,
//     pub transaction_id: Option<String>,
// }
