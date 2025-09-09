pub mod transformers;

use common_utils::{
    errors::CustomResult,
    ext_traits::BytesExt,
    request::{Method, Request, RequestBuilder, RequestContent},
};
use error_stack::ResultExt;
use hyperswitch_domain_models::{
    router_data::ErrorResponse,
    router_flow_types::{
        payments::{Authorize, Capture, PSync, Void, PaymentMethodToken, Session, SetupMandate},
        refunds::{Execute, RSync},
        access_token_auth::AccessTokenAuth,
    },
    router_request_types::{PaymentsAuthorizeData, PaymentsCancelData, PaymentsCaptureData, PaymentsSyncData, RefundsData, PaymentsSessionData, SetupMandateRequestData, PaymentMethodTokenizationData, AccessTokenRequestData},
    router_response_types::{PaymentsResponseData, RefundsResponseData},
    types::{PaymentsAuthorizeRouterData, PaymentsCancelRouterData, PaymentsCaptureRouterData, PaymentsSyncRouterData, RefundSyncRouterData, RefundsRouterData},
};
use hyperswitch_interfaces::{
    api::{
        self, ConnectorCommon, ConnectorIntegration, ConnectorSpecifications, ConnectorValidation,
        PaymentAuthorize,
    },
    configs::Connectors,
    consts::{NO_ERROR_CODE, NO_ERROR_MESSAGE},
    errors,
    events::connector_api_logs::ConnectorEvent,
    types::{PaymentsAuthorizeType, RefundExecuteType, Response},
    webhooks::{IncomingWebhook, IncomingWebhookRequestDetails},
};
use api_models::webhooks::{IncomingWebhookEvent, ObjectReferenceId};
use masking::{Mask, Maskable, PeekInterface};

use crate::{
    constants::headers,
    types::ResponseRouterData,
    utils::RefundsRequestData,
};

use self::transformers as wave;
use self::transformers::WaveCheckoutSessionResponse;

// Endpoints
const WAVE_BASE_URL: &str = "https://api.wave.com/";
const WAVE_CHECKOUT_SESSIONS: &str = "checkout/sessions";
const WAVE_CHECKOUT_SESSION_STATUS: &str = "checkout/sessions/{session_id}";
const WAVE_CANCEL_PAYMENT: &str = "v1/transactions/{txn_id}/cancel";
const WAVE_REFUND_FOR_TXN: &str = "v1/transactions/{txn_id}/refunds";
const WAVE_REFUND_STATUS: &str = "v1/refunds/{refund_id}";

#[derive(Debug, Clone)]
pub struct Wave;

impl Wave {
    pub const fn new() -> &'static Self {
        &Self
    }
}

impl ConnectorCommon for Wave {
    fn id(&self) -> &'static str {
        "wave"
    }

    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Minor
    }

    fn get_auth_header(
        &self,
        auth_type: &hyperswitch_domain_models::router_data::ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = wave::WaveAuthType::try_from(auth_type)?;
        Ok(vec![(
            headers::AUTHORIZATION.to_string(),
            format!("Bearer {}", auth.api_key.peek()).into_masked(),
        )])
    }

    fn base_url<'a>(&self, _connectors: &'a Connectors) -> &'a str {
        WAVE_BASE_URL
    }

    fn build_error_response(
        &self,
        res: Response,
        _event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: Result<wave::WaveErrorResponse, _> = res.response.parse_struct("WaveErrorResponse");
        match response {
            Ok(error_res) => Ok(ErrorResponse {
                code: error_res.code.unwrap_or_else(|| NO_ERROR_CODE.to_string()),
                message: error_res.message,
                reason: error_res.details.and_then(|d| d.first().map(|detail| detail.msg.clone())),
                status_code: res.status_code,
                attempt_status: None,
                connector_transaction_id: None,
                ..Default::default()
            }),
            Err(_) => Ok(ErrorResponse {
                code: NO_ERROR_CODE.to_string(),
                message: NO_ERROR_MESSAGE.to_string(),
                reason: Some("Failed to parse error response".to_string()),
                status_code: res.status_code,
                attempt_status: None,
                connector_transaction_id: None,
                ..Default::default()
            })
        }
    }
}

impl ConnectorSpecifications for Wave {}
impl ConnectorValidation for Wave {}

// Core trait implementations
impl api::Payment for Wave {}
impl api::PaymentSession for Wave {}
impl api::ConnectorAccessToken for Wave {}
impl api::MandateSetup for Wave {}
impl api::PaymentToken for Wave {}
impl api::PaymentSync for Wave {}
impl api::PaymentCapture for Wave {}
impl api::PaymentVoid for Wave {}
impl api::Refund for Wave {}
impl api::RefundExecute for Wave {}
impl api::RefundSync for Wave {}

// Default implementations for required ConnectorIntegration traits
impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Wave {}
impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData> for Wave {}
impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData> for Wave {}
impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, hyperswitch_domain_models::router_data::AccessToken> for Wave {}

// Payment flow implementations
impl PaymentAuthorize for Wave {}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &PaymentsAuthorizeRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let mut headers_vec = vec![(
            headers::CONTENT_TYPE.to_string(),
            PaymentsAuthorizeType::get_content_type(self).to_string().into(),
        )];
        let mut auth = self.get_auth_header(&req.connector_auth_type)?;
        headers_vec.append(&mut auth);
        Ok(headers_vec)
    }

    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}{}", self.base_url(connectors), WAVE_CHECKOUT_SESSIONS))
    }

    fn get_request_body(
        &self,
        req: &PaymentsAuthorizeRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let connector_router_data = wave::WaveRouterData::try_from((
            &self.get_currency_unit(),
            req.request.currency,
            req.request.minor_amount,
            req,
        ))?;
        let connector_req = wave::WaveCheckoutSessionRequest::try_from(&connector_router_data)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url(&self.get_url(req, connectors)?)
            .attach_default_headers()
            .headers(self.get_headers(req, connectors)?)
            .set_body(self.get_request_body(req, connectors)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &PaymentsAuthorizeRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PaymentsAuthorizeRouterData, errors::ConnectorError> {
        let response: WaveCheckoutSessionResponse = res
            .response
            .parse_struct("WaveCheckoutSessionResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        <PaymentsAuthorizeRouterData as TryFrom<ResponseRouterData<Authorize, WaveCheckoutSessionResponse, PaymentsAuthorizeData, PaymentsResponseData>>>::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }

    fn get_5xx_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

// Payment Sync implementation
impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &PaymentsSyncRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let mut headers_vec = vec![("Accept".to_string(), "application/json".to_string().into())];
        let mut auth = self.get_auth_header(&req.connector_auth_type)?;
        headers_vec.append(&mut auth);
        Ok(headers_vec)
    }

    fn get_url(
        &self,
        req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let connector_payment_id = req
            .request
            .connector_transaction_id
            .get_connector_transaction_id()
            .change_context(errors::ConnectorError::MissingConnectorTransactionID)?;
            
        Ok(format!(
            "{}{}",
            self.base_url(connectors),
            WAVE_CHECKOUT_SESSION_STATUS.replace("{session_id}", &connector_payment_id)
        ))
    }

    fn build_request(
        &self,
        req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Get)
                .url(&self.get_url(req, connectors)?)
                .attach_default_headers()
                .headers(self.get_headers(req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PaymentsSyncRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PaymentsSyncRouterData, errors::ConnectorError> {
        let response: wave::WavePaymentStatusResponse = res
            .response
            .parse_struct("WavePaymentStatusResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        <PaymentsSyncRouterData as TryFrom<ResponseRouterData<PSync, wave::WavePaymentStatusResponse, PaymentsSyncData, PaymentsResponseData>>>::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }

    fn get_5xx_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

// Payment Capture implementation - Wave uses automatic capture
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Wave {
    fn get_headers(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Payment Capture".to_string()).into())
    }

    fn get_url(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Payment Capture".to_string()).into())
    }

    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Payment Capture".to_string()).into())
    }

    fn handle_response(
        &self,
        _data: &PaymentsCaptureRouterData,
        _event_builder: Option<&mut ConnectorEvent>,
        _res: Response,
    ) -> CustomResult<PaymentsCaptureRouterData, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Payment Capture".to_string()).into())
    }

    fn get_error_response(
        &self,
        _res: Response,
        _event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Payment Capture".to_string()).into())
    }
}

// Payment Void implementation
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &PaymentsCancelRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let mut headers_vec = vec![("Accept".to_string(), "application/json".to_string().into())];
        let mut auth = self.get_auth_header(&req.connector_auth_type)?;
        headers_vec.append(&mut auth);
        Ok(headers_vec)
    }

    fn get_url(
        &self,
        req: &PaymentsCancelRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let connector_payment_id = req.request.connector_transaction_id.clone();
        Ok(format!(
            "{}{}",
            self.base_url(connectors),
            WAVE_CANCEL_PAYMENT.replace("{txn_id}", &connector_payment_id)
        ))
    }

    fn get_request_body(
        &self,
        req: &PaymentsCancelRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let connector_router_data = wave::WaveRouterData::try_from((
            &self.get_currency_unit(),
            req.request.currency.unwrap_or_default(),
            req.request.minor_amount.unwrap_or_default(),
            req,
        ))?;
        let connector_req = wave::WavePaymentsCancelRequest::try_from(&connector_router_data)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &PaymentsCancelRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url(&self.get_url(req, connectors)?)
            .attach_default_headers()
            .headers(self.get_headers(req, connectors)?)
            .set_body(self.get_request_body(req, connectors)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &PaymentsCancelRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PaymentsCancelRouterData, errors::ConnectorError> {
        let response: wave::WavePaymentsCancelResponse = res
            .response
            .parse_struct("WavePaymentsCancelResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        <PaymentsCancelRouterData as TryFrom<ResponseRouterData<Void, wave::WavePaymentsCancelResponse, PaymentsCancelData, PaymentsResponseData>>>::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }

    fn get_5xx_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

// Refund Execute implementation
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let mut headers_vec = vec![(
            headers::CONTENT_TYPE.to_string(),
            RefundExecuteType::get_content_type(self).to_string().into(),
        )];
        let mut auth = self.get_auth_header(&req.connector_auth_type)?;
        headers_vec.append(&mut auth);
        Ok(headers_vec)
    }

    fn get_url(
        &self,
        req: &RefundsRouterData<Execute>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let connector_payment_id = req.request.connector_transaction_id.clone();
        Ok(format!(
            "{}{}",
            self.base_url(connectors),
            WAVE_REFUND_FOR_TXN.replace("{txn_id}", &connector_payment_id)
        ))
    }

    fn get_request_body(
        &self,
        req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let connector_router_data = wave::WaveRouterData::try_from((
            &self.get_currency_unit(),
            req.request.currency,
            req.request.minor_refund_amount,
            req,
        ))?;
        let connector_req = wave::WaveRefundRequest::try_from(&connector_router_data)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &RefundsRouterData<Execute>,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url(&self.get_url(req, connectors)?)
            .attach_default_headers()
            .headers(self.get_headers(req, connectors)?)
            .set_body(self.get_request_body(req, connectors)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &RefundsRouterData<Execute>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<RefundsRouterData<Execute>, errors::ConnectorError> {
        let response: wave::WaveRefundResponse = res
            .response
            .parse_struct("WaveRefundResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        <RefundsRouterData<Execute> as TryFrom<crate::types::RefundsResponseRouterData<Execute, wave::WaveRefundResponse>>>::try_from(crate::types::RefundsResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }

    fn get_5xx_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

// Refund Sync implementation
impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &RefundSyncRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let mut headers_vec = vec![("Accept".to_string(), "application/json".to_string().into())];
        let mut auth = self.get_auth_header(&req.connector_auth_type)?;
        headers_vec.append(&mut auth);
        Ok(headers_vec)
    }

    fn get_url(
        &self,
        req: &RefundSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let connector_refund_id = req.request.get_connector_refund_id()?;
        Ok(format!(
            "{}{}",
            self.base_url(connectors),
            WAVE_REFUND_STATUS.replace("{refund_id}", &connector_refund_id)
        ))
    }

    fn build_request(
        &self,
        req: &RefundSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Get)
                .url(&self.get_url(req, connectors)?)
                .attach_default_headers()
                .headers(self.get_headers(req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &RefundSyncRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<RefundSyncRouterData, errors::ConnectorError> {
        let response: wave::WaveRefundResponse = res
            .response
            .parse_struct("WaveRefundResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        <RefundSyncRouterData as TryFrom<crate::types::RefundsResponseRouterData<RSync, wave::WaveRefundResponse>>>::try_from(crate::types::RefundsResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }

    fn get_5xx_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}







impl IncomingWebhook for Wave {
    fn get_webhook_object_reference_id(
        &self,
        _request: &IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<ObjectReferenceId, errors::ConnectorError> {
        Err(errors::ConnectorError::WebhooksNotImplemented.into())
    }

    fn get_webhook_event_type(
        &self,
        _request: &IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<IncomingWebhookEvent, errors::ConnectorError> {
        Err(errors::ConnectorError::WebhooksNotImplemented.into())
    }

    fn get_webhook_resource_object(
        &self,
        _request: &IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<Box<dyn masking::ErasedMaskSerialize>, errors::ConnectorError> {
        Err(errors::ConnectorError::WebhooksNotImplemented.into())
    }
}