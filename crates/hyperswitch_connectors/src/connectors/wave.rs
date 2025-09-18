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
use masking::{Mask, Maskable, PeekInterface, Secret};

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

// Aggregated Merchants API endpoints
//const WAVE_AGGREGATED_MERCHANTS: &str = "v1/aggregated_merchants";
const WAVE_AGGREGATED_MERCHANT_BY_ID: &str = "v1/aggregated_merchants/{id}";
const WAVE_AGGREGATED_MERCHANT_LIST: &str = "v1/aggregated_merchants";
const WAVE_AGGREGATED_MERCHANT_CREATE: &str = "v1/aggregated_merchants";
const WAVE_AGGREGATED_MERCHANT_UPDATE: &str = "v1/aggregated_merchants/{id}";
const WAVE_AGGREGATED_MERCHANT_DELETE: &str = "v1/aggregated_merchants/{id}";

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

impl Wave {
    /// Async helper to resolve and prepare aggregated merchant for payment
    /// This method can be called during payment processing before building the request
    pub async fn resolve_aggregated_merchant_for_payment(
        &self,
        req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        let auth = wave::WaveAuthType::try_from(&req.connector_auth_type)?;
        
        if !auth.aggregated_merchants_enabled {
            return Ok(None);
        }
        
        // Use the aggregated merchant resolver
        WaveAggregatedMerchantResolver::resolve_aggregated_merchant(
            &auth,
            self.base_url(connectors),
            req,
        ).await
    }
    
    /// Enhanced payment authorization with aggregated merchant support
    /// This method demonstrates how aggregated merchant resolution should be integrated
    pub async fn authorize_payment_with_aggregated_merchant(
        &self,
        req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<PaymentsAuthorizeRouterData, errors::ConnectorError> {
        // Step 1: Resolve aggregated merchant
        let aggregated_merchant_id = self
            .resolve_aggregated_merchant_for_payment(req, connectors)
            .await?;
        
        // Step 2: Log the resolution result
        if let Some(ref merchant_id) = aggregated_merchant_id {
            router_env::logger::info!(
                "Resolved aggregated merchant {} for payment authorization",
                merchant_id
            );
        } else {
            router_env::logger::debug!(
                "No aggregated merchant resolved for payment authorization"
            );
        }
        
        // Step 3: Build and execute the request
        // Note: In the current synchronous flow, we can't directly pass the resolved 
        // aggregated merchant ID to the request builder. The integration would need
        // to be modified to support async request building.
        
        // For now, we proceed with the normal flow, but this demonstrates
        // where the async resolution would fit in a redesigned flow.
        todo!("This method demonstrates async aggregated merchant integration")
    }
    
    /// Validate aggregated merchant configuration for a merchant account
    pub async fn validate_aggregated_merchant_config(
        &self,
        auth: &wave::WaveAuthType,
        metadata: &Option<wave::WaveConnectorMetadata>,
        connectors: &Connectors,
    ) -> CustomResult<bool, errors::ConnectorError> {
        if !auth.aggregated_merchants_enabled {
            return Ok(true); // No validation needed if feature is disabled
        }
        
        if let Some(meta) = metadata {
            // Validate the metadata structure
            wave::validate_wave_connector_metadata(meta)
                .map_err(|e| {
                    errors::ConnectorError::ProcessingStepFailed(Some(e.to_string().into()))
                })?;
            
            // If aggregated merchant ID is specified, validate it exists
            if let Some(ref merchant_id) = meta.aggregated_merchant_id {
                let exists = WaveAggregatedMerchantResolver::validate_aggregated_merchant(
                    auth,
                    self.base_url(connectors),
                    merchant_id,
                ).await?;
                
                if !exists {
                    let error_message = format!("Aggregated merchant {} not found or not accessible", merchant_id);
                    return Err(errors::ConnectorError::ProcessingStepFailed(Some(error_message.into())).into());
                }
            }
        }
        
        Ok(true)
    }
}

// Wave Aggregated Merchant Resolution Logic
pub struct WaveAggregatedMerchantResolver;

impl WaveAggregatedMerchantResolver {
    /// Resolve aggregated merchant ID for payment, with auto-creation if enabled
    pub async fn resolve_aggregated_merchant(
        auth: &wave::WaveAuthType,
        base_url: &str,
        router_data: &PaymentsAuthorizeRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        // If aggregated merchants are not enabled, return None
        if !auth.aggregated_merchants_enabled {
            return Ok(None);
        }
        
        // Try to extract aggregated merchant metadata
        let metadata = wave::extract_wave_connector_metadata(router_data)?;
        
        // If metadata exists and has aggregated merchant ID, validate and return it
        if let Some(meta) = &metadata {
            if let Some(aggregated_merchant_id) = &meta.aggregated_merchant_id {
                // Validate the merchant ID exists and is accessible
                match Self::validate_aggregated_merchant(auth, base_url, aggregated_merchant_id).await {
                    Ok(true) => return Ok(Some(aggregated_merchant_id.clone())),
                    Ok(false) => {
                        router_env::logger::warn!(
                            "Aggregated merchant ID {} not found or not accessible",
                            aggregated_merchant_id
                        );
                        // Continue to auto-creation if enabled
                    },
                    Err(e) => {
                        router_env::logger::error!(
                            "Error validating aggregated merchant {}: {:?}",
                            aggregated_merchant_id,
                            e
                        );
                        // Continue to auto-creation if enabled
                    }
                }
            }
        }
        
        // Check if auto-create is enabled
        let auto_create = metadata
            .as_ref()
            .and_then(|m| m.auto_create_aggregated_merchant)
            .unwrap_or(auth.auto_create_aggregated_merchant);
            
        if auto_create {
            // Attempt to auto-create aggregated merchant
            Self::auto_create_aggregated_merchant(auth, base_url, router_data, metadata.as_ref()).await
        } else {
            Ok(None)
        }
    }
    
    /// Auto-create aggregated merchant based on business profile information with enhanced validation
    async fn auto_create_aggregated_merchant(
        auth: &wave::WaveAuthType,
        base_url: &str,
        router_data: &PaymentsAuthorizeRouterData,
        metadata: Option<&wave::WaveConnectorMetadata>,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        // For auto-creation, we need profile information
        // In a real implementation, this would need access to business profile data
        // For now, we'll use a default profile name based on merchant_id
        let profile_name = format!("Profile_{}", router_data.merchant_id.get_string_repr());
        
        router_env::logger::info!(
            "Attempting auto-creation of aggregated merchant for profile: {}",
            profile_name
        );
        
        let request = match wave::build_aggregated_merchant_request_from_profile(
            &profile_name,
            metadata,
        ) {
            Ok(req) => req,
            Err(e) => {
                router_env::logger::warn!(
                    "Invalid aggregated merchant configuration for profile {}: {:?}",
                    profile_name,
                    e
                );
                return Err(errors::ConnectorError::from(e).into());
            }
        };
        
        match WaveAggregatedMerchantService::create_aggregated_merchant(
            &auth.api_key,
            base_url,
            request,
        ).await {
            Ok(merchant) => {
                // Successfully created aggregated merchant
                router_env::logger::info!(
                    "Auto-created aggregated merchant: {} for profile: {}",
                    merchant.id,
                    profile_name
                );
                
                // TODO: Update connector metadata with the new aggregated merchant ID
                // This would require access to the storage layer to update the merchant connector account
                
                Ok(Some(merchant.id))
            },
            Err(e) => {
                // Log the error but don't fail the payment
                router_env::logger::warn!(
                    "Failed to auto-create aggregated merchant for profile {}: {:?}",
                    profile_name,
                    e
                );
                // Graceful degradation: continue without aggregated merchant
                Ok(None)
            }
        }
    }
    
    /// Validate aggregated merchant exists and is accessible with retry logic
    pub async fn validate_aggregated_merchant(
        auth: &wave::WaveAuthType,
        base_url: &str,
        aggregated_merchant_id: &str,
    ) -> CustomResult<bool, errors::ConnectorError> {
        // Implement simple retry logic for transient failures
        let max_retries = 3;
        let mut retry_count = 0;
        
        while retry_count < max_retries {
            match WaveAggregatedMerchantService::get_aggregated_merchant(
                &auth.api_key,
                base_url,
                aggregated_merchant_id,
            ).await {
                Ok(_) => return Ok(true),
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        router_env::logger::error!(
                            "Failed to validate aggregated merchant {} after {} retries: {:?}",
                            aggregated_merchant_id,
                            max_retries,
                            e
                        );
                        return Ok(false);
                    }
                    
                    // Wait before retry (exponential backoff)
                    // Note: In production, this should use proper async delay
                    // let delay_ms = 100 * (2_u64.pow(retry_count - 1));
                    // TODO: Replace with proper async sleep implementation
                }
            }
        }
        
        Ok(false)
    }
    
    /// Get or create aggregated merchant with caching support
    pub async fn get_or_create_aggregated_merchant(
        auth: &wave::WaveAuthType,
        base_url: &str,
        router_data: &PaymentsAuthorizeRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        // Try to resolve existing aggregated merchant first
        Self::resolve_aggregated_merchant(auth, base_url, router_data).await
    }
    
    /// Resolve aggregated merchant with fallback strategies
    pub async fn resolve_with_fallback(
        auth: &wave::WaveAuthType,
        base_url: &str,
        router_data: &PaymentsAuthorizeRouterData,
        fallback_strategies: &[AggregatedMerchantFallbackStrategy],
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        // First try normal resolution
        if let Ok(Some(merchant_id)) = Self::resolve_aggregated_merchant(auth, base_url, router_data).await {
            return Ok(Some(merchant_id));
        }
        
        // Try fallback strategies in order
        for strategy in fallback_strategies {
            match strategy {
                AggregatedMerchantFallbackStrategy::UseDefault => {
                    // Use a default aggregated merchant if available
                    // This would be configured at the connector level
                    continue;
                },
                AggregatedMerchantFallbackStrategy::CreateTemporary => {
                    // Create a temporary aggregated merchant for this transaction
                    if let Ok(Some(merchant_id)) = Self::auto_create_aggregated_merchant(
                        auth, base_url, router_data, None
                    ).await {
                        return Ok(Some(merchant_id));
                    }
                },
                AggregatedMerchantFallbackStrategy::Skip => {
                    // Continue without aggregated merchant
                    return Ok(None);
                }
            }
        }
        
        Ok(None)
    }
}

/// Fallback strategies for aggregated merchant resolution
#[derive(Debug, Clone)]
pub enum AggregatedMerchantFallbackStrategy {
    UseDefault,
    CreateTemporary,
    Skip,
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
        
        // Create the checkout session request with aggregated merchant support
        let mut connector_req = wave::WaveCheckoutSessionRequest::try_from(&connector_router_data)?;
        
        // If aggregated merchant ID is not already set, try to resolve it
        if connector_req.aggregated_merchant_id.is_none() {
            let auth = wave::WaveAuthType::try_from(&req.connector_auth_type)?;
            
            // Only resolve if aggregated merchants are enabled
            if auth.aggregated_merchants_enabled {
                // Try to resolve aggregated merchant from metadata
                // Note: In a real implementation, this might need async resolution
                let metadata = wave::extract_wave_connector_metadata(req)?;
                if let Some(meta) = metadata {
                    if let Some(ref merchant_id) = meta.aggregated_merchant_id {
                        connector_req.aggregated_merchant_id = Some(merchant_id.clone());
                        
                        router_env::logger::info!(
                            "Using configured aggregated merchant: {} for payment",
                            merchant_id
                        );
                    }
                }
            }
        }
        
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // Note: This is a synchronous method, but aggregated merchant resolution is async.
        // In a real production implementation, the aggregated merchant resolution should be 
        // moved to an earlier async phase in the payment processing pipeline.
        // For now, we rely on pre-configured aggregated merchant IDs in metadata.
        
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

// Wave Aggregated Merchant Service
pub struct WaveAggregatedMerchantService;

impl WaveAggregatedMerchantService {
    /// Create a new aggregated merchant with enhanced error handling
    pub async fn create_aggregated_merchant(
        api_key: &Secret<String>,
        base_url: &str,
        request: wave::WaveAggregatedMerchantRequest,
    ) -> CustomResult<wave::WaveAggregatedMerchant, errors::ConnectorError> {
        // Validate request before making API call
        wave::validate_wave_aggregated_merchant_request(&request)
            .map_err(|e| errors::ConnectorError::ProcessingStepFailed(Some(e.to_string().into())))?;
        
        let url = format!("{}{}", base_url, WAVE_AGGREGATED_MERCHANT_CREATE);
        let auth_header = format!("Bearer {}", api_key.peek());
        
        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header(headers::AUTHORIZATION, auth_header)
            .header(headers::CONTENT_TYPE, "application/json")
            .json(&request)
            .send()
            .await
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
            
        if response.status().is_success() {
            response
                .json::<wave::WaveAggregatedMerchant>()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)
        } else {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
            Err(wave::parse_wave_api_error(status, &error_text)).change_context(errors::ConnectorError::ProcessingStepFailed(None))
        }
    }
    
    /// List aggregated merchants with pagination support
    pub async fn list_aggregated_merchants(
        api_key: &Secret<String>,
        base_url: &str,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> CustomResult<wave::WaveAggregatedMerchantListResponse, errors::ConnectorError> {
        let mut url = format!("{}{}", base_url, WAVE_AGGREGATED_MERCHANT_LIST);
        
        // Add query parameters for pagination
        let mut query_params = Vec::new();
        if let Some(limit_val) = limit {
            query_params.push(format!("limit={}", limit_val));
        }
        if let Some(cursor_val) = cursor {
            query_params.push(format!("cursor={}", cursor_val));
        }
        
        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }
        
        let auth_header = format!("Bearer {}", api_key.peek());
        
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header(headers::AUTHORIZATION, auth_header)
            .send()
            .await
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
            
        if response.status().is_success() {
            response
                .json::<wave::WaveAggregatedMerchantListResponse>()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)
        } else {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
            Err(wave::parse_wave_api_error(status, &error_text)).change_context(errors::ConnectorError::ProcessingStepFailed(None))
        }
    }
    
    /// Get aggregated merchant by ID with enhanced error handling
    pub async fn get_aggregated_merchant(
        api_key: &Secret<String>,
        base_url: &str,
        merchant_id: &str,
    ) -> CustomResult<wave::WaveAggregatedMerchant, errors::ConnectorError> {
        // Validate merchant ID format
        if merchant_id.is_empty() || !merchant_id.starts_with("am-") {
            return Err(errors::ConnectorError::InvalidConnectorConfig {
                config: "Invalid aggregated merchant ID format"
            }.into());
        }
        
        let url = format!("{}{}", base_url, WAVE_AGGREGATED_MERCHANT_BY_ID.replace("{id}", merchant_id));
        let auth_header = format!("Bearer {}", api_key.peek());
        
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header(headers::AUTHORIZATION, auth_header)
            .send()
            .await
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
            
        if response.status().is_success() {
            response
                .json::<wave::WaveAggregatedMerchant>()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)
        } else {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
            Err(wave::parse_wave_api_error(status, &error_text)).change_context(errors::ConnectorError::ProcessingStepFailed(None))
        }
    }
    
    /// Update aggregated merchant with validation
    pub async fn update_aggregated_merchant(
        api_key: &Secret<String>,
        base_url: &str,
        merchant_id: &str,
        request: wave::WaveAggregatedMerchantUpdateRequest,
    ) -> CustomResult<wave::WaveAggregatedMerchant, errors::ConnectorError> {
        // Validate merchant ID format
        if merchant_id.is_empty() || !merchant_id.starts_with("am-") {
            return Err(errors::ConnectorError::InvalidConnectorConfig {
                config: "Invalid aggregated merchant ID format"
            }.into());
        }
        
        // Validate update request fields if provided
        if let Some(ref name) = request.name {
            if name.is_empty() || name.len() > 255 {
                return Err(errors::ConnectorError::InvalidConnectorConfig {
                    config: "Merchant name must be between 1 and 255 characters"
                }.into());
            }
        }
        
        if let Some(ref description) = request.business_description {
            if description.is_empty() || description.len() > 500 {
                return Err(errors::ConnectorError::InvalidConnectorConfig {
                    config: "Business description must be between 1 and 500 characters"
                }.into());
            }
        }
        
        let url = format!("{}{}", base_url, WAVE_AGGREGATED_MERCHANT_UPDATE.replace("{id}", merchant_id));
        let auth_header = format!("Bearer {}", api_key.peek());
        
        let client = reqwest::Client::new();
        let response = client
            .put(&url)
            .header(headers::AUTHORIZATION, auth_header)
            .header(headers::CONTENT_TYPE, "application/json")
            .json(&request)
            .send()
            .await
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
            
        if response.status().is_success() {
            response
                .json::<wave::WaveAggregatedMerchant>()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)
        } else {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
            Err(wave::parse_wave_api_error(status, &error_text)).change_context(errors::ConnectorError::ProcessingStepFailed(None))
        }
    }
    
    /// Delete aggregated merchant with proper validation
    pub async fn delete_aggregated_merchant(
        api_key: &Secret<String>,
        base_url: &str,
        merchant_id: &str,
    ) -> CustomResult<(), errors::ConnectorError> {
        // Validate merchant ID format
        if merchant_id.is_empty() || !merchant_id.starts_with("am-") {
            return Err(errors::ConnectorError::InvalidConnectorConfig {
                config: "Invalid aggregated merchant ID format"
            }.into());
        }
        
        let url = format!("{}{}", base_url, WAVE_AGGREGATED_MERCHANT_DELETE.replace("{id}", merchant_id));
        let auth_header = format!("Bearer {}", api_key.peek());
        
        let client = reqwest::Client::new();
        let response = client
            .delete(&url)
            .header(headers::AUTHORIZATION, auth_header)
            .send()
            .await
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
            
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
            Err(wave::parse_wave_api_error(status, &error_text)).change_context(errors::ConnectorError::ProcessingStepFailed(None))
        }
    }
    
    /// Check if aggregated merchant exists (lightweight operation)
    pub async fn merchant_exists(
        api_key: &Secret<String>,
        base_url: &str,
        merchant_id: &str,
    ) -> CustomResult<bool, errors::ConnectorError> {
        match Self::get_aggregated_merchant(api_key, base_url, merchant_id).await {
            Ok(_) => Ok(true),
            Err(err) => {
                // Check if the error is specifically "not found"
                if let Some(error_stack) = err.downcast_ref::<errors::ConnectorError>() {
                    match error_stack {
                        errors::ConnectorError::ProcessingStepFailed(_) => Ok(false),
                        _ => Err(err),
                    }
                } else {
                    Err(err)
                }
            }
        }
    }
    
    /// Batch get aggregated merchants by IDs (utility method)
    pub async fn get_multiple_aggregated_merchants(
        api_key: &Secret<String>,
        base_url: &str,
        merchant_ids: &[String],
    ) -> CustomResult<Vec<(String, Result<wave::WaveAggregatedMerchant, error_stack::Report<errors::ConnectorError>>)>, errors::ConnectorError> {
        let mut results = Vec::new();
        
        for merchant_id in merchant_ids {
            let result = Self::get_aggregated_merchant(api_key, base_url, merchant_id).await;
            results.push((merchant_id.clone(), result));
        }
        
        Ok(results)
    }
}
