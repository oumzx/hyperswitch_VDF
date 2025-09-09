use common_enums::{enums as api_enums, AttemptStatus, RefundStatus};
use common_utils::{
    pii::Email,
    request::Method,
    types::MinorUnit,
};
use hyperswitch_domain_models::{
    router_data::{ConnectorAuthType, RouterData},
    router_flow_types::{Execute},
    router_request_types::{ResponseId},
    router_response_types::{PaymentsResponseData, RefundsResponseData, RedirectForm},
    types::{
        PaymentsAuthorizeRouterData, PaymentsCancelRouterData, RefundsRouterData,
    },
};
use hyperswitch_interfaces::{
    api, 
    errors::ConnectorError,
};
use masking::Secret;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    types::{RefundsResponseRouterData, ResponseRouterData},
    utils::{PaymentsAuthorizeRequestData, RouterData as UtilsRouterData},
};

pub struct WaveRouterData<T> {
    pub amount: MinorUnit,
    pub router_data: T,
}

impl<T> TryFrom<(&api::CurrencyUnit, api_enums::Currency, MinorUnit, T)> for WaveRouterData<T> {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        (currency_unit, _currency, amount, item): (
            &api::CurrencyUnit,
            api_enums::Currency,
            MinorUnit,
            T,
        ),
    ) -> Result<Self, Self::Error> {
        let amount = match currency_unit {
            api::CurrencyUnit::Base => amount,
            api::CurrencyUnit::Minor => amount,
        };
        Ok(Self {
            amount,
            router_data: item,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WaveAuthType {
    pub api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for WaveAuthType {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::HeaderKey { api_key } => Ok(Self {
                api_key: api_key.to_owned(),
            }),
            _ => Err(ConnectorError::FailedToObtainAuthType.into()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WaveCheckoutSessionRequest {
    pub amount: String,
    pub currency: String,
    pub error_url: Option<String>,
    pub success_url: Option<String>,
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer: Option<WaveCustomer>,
}

#[derive(Debug, Serialize)]
pub struct WaveCustomer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Secret<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<Email>,
}

impl TryFrom<&WaveRouterData<&PaymentsAuthorizeRouterData>> for WaveCheckoutSessionRequest {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: &WaveRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        let amount = item.amount.to_string();
        let currency = router_data.request.currency.to_string();
        
        let return_url = router_data.request.get_router_return_url()?;
        
        let customer = router_data.request.email.as_ref().map(|email| WaveCustomer {
            name: router_data.get_billing_address()
                .ok()
                .and_then(|billing| billing.get_optional_full_name()),
            email: Some(email.clone()),
        });

        Ok(Self {
            amount,
            currency,
            error_url: Some(return_url.clone()),
            success_url: Some(return_url),
            reference: Some(router_data.connector_request_reference_id.clone()),
            customer,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WaveCheckoutSessionResponse {
    pub id: String,
    pub launch_url: Option<String>,
    pub status: WavePaymentStatus,
    pub amount: String,
    pub currency: String,
    pub reference: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WavePaymentStatus {
    Created,
    Pending,
    Completed,
    Failed,
    Cancelled,
}

impl From<WavePaymentStatus> for AttemptStatus {
    fn from(status: WavePaymentStatus) -> Self {
        match status {
            WavePaymentStatus::Created | WavePaymentStatus::Pending => Self::Pending,
            WavePaymentStatus::Completed => Self::Charged,
            WavePaymentStatus::Failed => Self::Failure,
            WavePaymentStatus::Cancelled => Self::Voided,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WavePaymentsCancelResponse {
    pub id: String,
    pub status: WavePaymentStatus,
}

#[derive(Debug, Serialize)]
pub struct WavePaymentsCancelRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl TryFrom<&WaveRouterData<&PaymentsCancelRouterData>> for WavePaymentsCancelRequest {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: &WaveRouterData<&PaymentsCancelRouterData>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            reason: item.router_data.request.cancellation_reason.clone(),
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WavePaymentStatusResponse {
    pub id: String,
    pub status: WavePaymentStatus,
    pub amount: String,
    pub currency: String,
    pub reference: Option<String>,
    pub launch_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WaveRefundRequest {
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl TryFrom<&WaveRouterData<&RefundsRouterData<Execute>>> for WaveRefundRequest {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: &WaveRouterData<&RefundsRouterData<Execute>>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            amount: item.amount.to_string(),
            reason: item.router_data.request.reason.clone(),
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WaveRefundResponse {
    pub id: String,
    pub status: WaveRefundStatus,
    pub amount: String,
    pub currency: String,
    pub transaction_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WaveRefundStatus {
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl From<WaveRefundStatus> for RefundStatus {
    fn from(status: WaveRefundStatus) -> Self {
        match status {
            WaveRefundStatus::Processing => Self::Pending,
            WaveRefundStatus::Completed => Self::Success,
            WaveRefundStatus::Failed => Self::Failure,
            WaveRefundStatus::Cancelled => Self::Failure,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WaveErrorResponse {
    pub code: Option<String>,
    pub message: String,
    pub details: Option<Vec<WaveErrorDetail>>,
}

#[derive(Debug, Deserialize)]
pub struct WaveErrorDetail {
    pub loc: Option<Vec<String>>,
    pub msg: String,
}

// Response transformations

impl<F, T>
    TryFrom<ResponseRouterData<F, WaveCheckoutSessionResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, WaveCheckoutSessionResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let status = AttemptStatus::from(item.response.status.clone());
        let redirection_data = item.response.launch_url.and_then(|url_str| {
            Url::parse(&url_str)
                .map(|url| RedirectForm::from((url, Method::Get)))
                .ok()
        });

        Ok(Self {
            status,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(
                    item.response.id.clone(),
                ),
                redirection_data: Box::new(redirection_data),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                connector_response_reference_id: item.response.reference,
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

impl<F, T>
    TryFrom<ResponseRouterData<F, WavePaymentsCancelResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, WavePaymentsCancelResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let status = AttemptStatus::from(item.response.status);
        Ok(Self {
            status,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(
                    item.response.id,
                ),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                connector_response_reference_id: None,
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

impl<F, T>
    TryFrom<ResponseRouterData<F, WavePaymentStatusResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, WavePaymentStatusResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let status = AttemptStatus::from(item.response.status.clone());
        let redirection_data = item.response.launch_url.and_then(|url_str| {
            Url::parse(&url_str)
                .map(|url| RedirectForm::from((url, Method::Get)))
                .ok()
        });

        Ok(Self {
            status,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(
                    item.response.id,
                ),
                redirection_data: Box::new(redirection_data),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                connector_response_reference_id: item.response.reference,
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

impl<F> TryFrom<RefundsResponseRouterData<F, WaveRefundResponse>> for RefundsRouterData<F> {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(
        item: RefundsResponseRouterData<F, WaveRefundResponse>,
    ) -> Result<Self, Self::Error> {
        let refund_status = RefundStatus::from(item.response.status);
        Ok(Self {
            response: Ok(RefundsResponseData {
                connector_refund_id: item.response.id,
                refund_status,
            }),
            ..item.data
        })
    }
}

// Webhook data structures

#[derive(Debug, Deserialize)]
pub struct WaveWebhookPayload {
    pub event_type: String,
    pub data: WaveWebhookData,
    pub timestamp: String,
}

#[derive(Debug, Deserialize)]
pub struct WaveWebhookData {
    pub session_id: Option<String>,
    pub transaction_id: Option<String>,
    pub status: WavePaymentStatus,
    pub amount: String,
    pub currency: String,
    pub reference: Option<String>,
}
