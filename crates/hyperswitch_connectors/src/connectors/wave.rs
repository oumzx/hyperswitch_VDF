// crates/hyperswitch_connectors/src/connectors/wave.rs

pub mod transformers; // placeholder

use common_utils::{
    errors::CustomResult,
    request::{Method, Request, RequestBuilder, RequestContent},
};
use hyperswitch_domain_models::{
    router_data::ErrorResponse,
    router_flow_types::{
        payments::{Authorize, PSync, Void},
        refunds::{Execute, RSync},
    },
    router_request_types::{PaymentsAuthorizeData, PaymentsCancelData, PaymentsSyncData, RefundsData},
    router_response_types::{PaymentsResponseData, RefundsResponseData},
    router_data::RouterData as FlowRouterData,
    types::{
        PaymentsAuthorizeRouterData, PaymentsCancelRouterData, PaymentsSyncRouterData,
        RefundsRouterData,
    },
};
use hyperswitch_interfaces::{
    api::{
        self, ConnectorCommon, ConnectorIntegration, ConnectorSpecifications, ConnectorValidation,
        PaymentAuthorize, PaymentSync, PaymentVoid, Refund, RefundExecute, RefundSync,
    },
    configs::Connectors,
    consts::{NO_ERROR_CODE, NO_ERROR_MESSAGE},
    errors,
    events::connector_api_logs::ConnectorEvent,
    types::{PaymentsAuthorizeType, RefundExecuteType, Response},
};
use masking::Maskable;

use crate::constants::headers;

// Endpoints basiques (à déplacer vers la config si besoin)
const WAVE_BASE_URL: &str = "https://api.wave.com/";
const WAVE_CHECKOUT_SESSIONS: &str = "checkout/sessions";
const WAVE_REFUND_FOR_TXN: &str = "v1/transactions/{txn_id}/refunds";

#[derive(Debug, Clone)]
pub struct Wave;

// ========== ConnectorCommon ==========

impl ConnectorCommon for Wave {
    fn id(&self) -> &'static str {
        "wave"
    }

    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Minor
    }

    fn get_auth_header(
        &self,
        _auth_type: &hyperswitch_domain_models::router_data::ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        // À ajuster quand on aura le vrai schéma d’auth Wave
        Ok(vec![])
    }

    fn base_url<'a>(&self, _connectors: &'a Connectors) -> &'a str {
        WAVE_BASE_URL
    }

    fn build_error_response(
        &self,
        _res: Response,
        _event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        Ok(ErrorResponse {
            code: NO_ERROR_CODE.to_string(),
            message: NO_ERROR_MESSAGE.to_string(),
            reason: Some("wave: unable to parse error response".to_string()),
            status_code: 0,
            attempt_status: None,
            connector_transaction_id: None,
            ..Default::default()
        })
    }
}

// ========== ConnectorSpecifications & Validation ==========

impl ConnectorSpecifications for Wave {}
impl ConnectorValidation for Wave {}

// ========== Marker traits requis par `api::Payment` ==========

// impl PaymentToken for Wave {}
// impl PaymentSession for Wave {}
// impl MandateSetup for Wave {}
// impl PaymentCapture for Wave {}
// impl Payment for Wave {}

// ========== Payment Authorize ==========

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
        _req: &PaymentsAuthorizeRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        // Corps minimal — on l’adaptera avec transformers.rs
        let body = serde_json::json!({});
        Ok(RequestContent::Json(Box::new(body)))
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
        _data: &PaymentsAuthorizeRouterData,
        _event_builder: Option<&mut ConnectorEvent>,
        _res: Response,
    ) -> CustomResult<PaymentsAuthorizeRouterData, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented(
            "Wave authorize handle_response".to_string(),
        )
        .into())
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

// ========== Void (Cancel) ==========

impl PaymentVoid for Wave {}

impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &PaymentsCancelRouterData,
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
        _req: &PaymentsCancelRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}{}", self.base_url(connectors), "payments/cancel"))
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

// ========== Payment Sync (PSync) — squelette ==========

impl PaymentSync for Wave {}

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Wave {
    fn get_headers(
        &self,
        req: &FlowRouterData<PSync, PaymentsSyncData, PaymentsResponseData>,
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
        _req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}{}", self.base_url(connectors), "payments/sync"))
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

// ========== Refund Execute ==========

impl Refund for Wave {}
impl RefundExecute for Wave {}
impl RefundSync for Wave {}

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
        let txn_id = req.request.connector_transaction_id.clone();
        Ok(format!(
            "{}{}",
            self.base_url(connectors),
            WAVE_REFUND_FOR_TXN.replace("{txn_id}", &txn_id)
        ))
    }

    fn get_request_body(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let body = serde_json::json!({});
        Ok(RequestContent::Json(Box::new(body)))
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
        _data: &RefundsRouterData<Execute>,
        _event_builder: Option<&mut ConnectorEvent>,
        _res: Response,
    ) -> CustomResult<RefundsRouterData<Execute>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented(
            "Wave refund execute handle_response".to_string(),
        )
        .into())
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

// ========== Refund Sync — squelette ==========

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Wave {
    fn get_url(
        &self,
        _req: &RefundsRouterData<RSync>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}{}", self.base_url(connectors), "refunds/sync"))
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
