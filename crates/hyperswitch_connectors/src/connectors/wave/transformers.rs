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
use masking::{Secret, PeekInterface};
use serde::{Deserialize, Serialize};
use url::Url;


use crate::{
    types::{RefundsResponseRouterData, ResponseRouterData},
    utils::{PaymentsAuthorizeRequestData, RouterData as UtilsRouterData},
};

// Business types supported by Wave for aggregated merchants
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WaveBusinessType {
    Ecommerce,
    Mobile,
    Pos,
    Marketplace,
    Subscription,
    Other,
}

impl Default for WaveBusinessType {
    fn default() -> Self {
        Self::Ecommerce
    }
}

// Enhanced Wave authentication configuration for aggregated merchants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveAggregatedMerchantConfig {
    pub enabled: bool,
    pub auto_create_on_profile_creation: bool,
    pub default_business_type: WaveBusinessType,
    pub cache_ttl_seconds: u64,
}

impl Default for WaveAggregatedMerchantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_create_on_profile_creation: false,
            default_business_type: WaveBusinessType::default(),
            cache_ttl_seconds: 3600, // 1 hour
        }
    }
}

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
    pub aggregated_merchants_enabled: bool,
    pub auto_create_aggregated_merchant: bool,
    pub default_business_type: WaveBusinessType,
    pub cache_ttl_seconds: u64,
}

impl TryFrom<&ConnectorAuthType> for WaveAuthType {
    type Error = error_stack::Report<ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::HeaderKey { api_key } => Ok(Self {
                api_key: api_key.to_owned(),
                aggregated_merchants_enabled: false, // Default to false for backward compatibility
                auto_create_aggregated_merchant: false,
                default_business_type: WaveBusinessType::default(),
                cache_ttl_seconds: 3600, // 1 hour default cache TTL
            }),
            ConnectorAuthType::BodyKey { api_key, key1 } => {
                // Support enhanced configuration via key1 field
                let enhanced_config = serde_json::from_str::<WaveAggregatedMerchantConfig>(key1.peek())
                    .ok()
                    .unwrap_or_default();
                
                Ok(Self {
                    api_key: api_key.to_owned(),
                    aggregated_merchants_enabled: enhanced_config.enabled,
                    auto_create_aggregated_merchant: enhanced_config.auto_create_on_profile_creation,
                    default_business_type: enhanced_config.default_business_type,
                    cache_ttl_seconds: enhanced_config.cache_ttl_seconds,
                })
            },
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
    pub aggregated_merchant_id: Option<String>, // New field for aggregated merchant support
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
        
        // Extract aggregated merchant ID from connector metadata with enhanced logic
        let aggregated_merchant_id = extract_aggregated_merchant_id(router_data)
            .unwrap_or(None);
        
        // Log aggregated merchant usage for monitoring
        if aggregated_merchant_id.is_some() {
            router_env::logger::info!(
                "Using aggregated merchant for payment: merchant_id={}", 
                router_data.merchant_id.get_string_repr()
            );
        }
        
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
            aggregated_merchant_id, // Include aggregated merchant ID
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

// Wave aggregated merchant structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveAggregatedMerchant {
    pub id: String,
    pub name: String,
    pub business_type: WaveBusinessType,
    pub business_registration_identifier: Option<String>,
    pub business_sector: Option<String>,
    pub website_url: Option<String>,
    pub business_description: String,
    pub manager_name: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveAggregatedMerchantRequest {
    pub name: String,
    pub business_type: WaveBusinessType,
    pub business_registration_identifier: Option<String>,
    pub business_sector: Option<String>,
    pub website_url: Option<String>,
    pub business_description: String,
    pub manager_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveAggregatedMerchantUpdateRequest {
    pub name: Option<String>,
    pub business_type: Option<WaveBusinessType>,
    pub business_registration_identifier: Option<String>,
    pub business_sector: Option<String>,
    pub website_url: Option<String>,
    pub business_description: Option<String>,
    pub manager_name: Option<String>,
}

// Enhanced error handling for aggregated merchant operations
#[derive(Debug, Clone)]
pub enum WaveAggregatedMerchantError {
    MerchantNotFound { merchant_id: String },
    CreationFailed { reason: String },
    InvalidConfiguration { details: String },
    ValidationFailed { merchant_id: String },
    AutoCreationDisabled,
    RateLimitExceeded,
    AuthenticationFailed,
}

impl std::fmt::Display for WaveAggregatedMerchantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveAggregatedMerchantError::MerchantNotFound { merchant_id } => {
                write!(f, "Aggregated merchant not found: {}", merchant_id)
            }
            WaveAggregatedMerchantError::CreationFailed { reason } => {
                write!(f, "Aggregated merchant creation failed: {}", reason)
            }
            WaveAggregatedMerchantError::InvalidConfiguration { details } => {
                write!(f, "Invalid aggregated merchant configuration: {}", details)
            }
            WaveAggregatedMerchantError::ValidationFailed { merchant_id } => {
                write!(f, "Aggregated merchant validation failed: {}", merchant_id)
            }
            WaveAggregatedMerchantError::AutoCreationDisabled => {
                write!(f, "Auto-creation disabled for aggregated merchants")
            }
            WaveAggregatedMerchantError::RateLimitExceeded => {
                write!(f, "Aggregated merchant API limit exceeded")
            }
            WaveAggregatedMerchantError::AuthenticationFailed => {
                write!(f, "Authentication failed for aggregated merchant operations")
            }
        }
    }
}

impl std::error::Error for WaveAggregatedMerchantError {}

impl From<WaveAggregatedMerchantError> for ConnectorError {
    fn from(error: WaveAggregatedMerchantError) -> Self {
        match error {
            WaveAggregatedMerchantError::MerchantNotFound { .. } => {
                ConnectorError::ProcessingStepFailed(Some(error.to_string().into()))
            }
            WaveAggregatedMerchantError::CreationFailed { .. } => {
                ConnectorError::ProcessingStepFailed(Some(error.to_string().into()))
            }
            WaveAggregatedMerchantError::InvalidConfiguration { .. } => {
                ConnectorError::ProcessingStepFailed(Some(error.to_string().into()))
            }
            WaveAggregatedMerchantError::ValidationFailed { .. } => {
                ConnectorError::ProcessingStepFailed(Some(error.to_string().into()))
            }
            WaveAggregatedMerchantError::AutoCreationDisabled => {
                ConnectorError::ProcessingStepFailed(Some(error.to_string().into()))
            }
            WaveAggregatedMerchantError::RateLimitExceeded => {
                ConnectorError::ProcessingStepFailed(Some(error.to_string().into()))
            }
            WaveAggregatedMerchantError::AuthenticationFailed => {
                ConnectorError::FailedToObtainAuthType
            }
        }
    }
}

/// Parse Wave API error response and convert to appropriate error
pub fn parse_wave_api_error(status: u16, body: &str) -> ConnectorError {
    // Try to parse as Wave error response
    if let Ok(error_response) = serde_json::from_str::<WaveErrorResponse>(body) {
        let error_code = error_response.code.unwrap_or_default();
        let error_message = error_response.message;
        
        match (status, error_code.as_str()) {
            (404, "AGGREGATED_MERCHANT_NOT_FOUND") => {
                WaveAggregatedMerchantError::MerchantNotFound {
                    merchant_id: "unknown".to_string(),
                }.into()
            }
            (400, "INVALID_BUSINESS_TYPE") => {
                WaveAggregatedMerchantError::InvalidConfiguration {
                    details: error_message,
                }.into()
            }
            (401, _) | (403, _) => {
                WaveAggregatedMerchantError::AuthenticationFailed.into()
            }
            (429, _) => {
                WaveAggregatedMerchantError::RateLimitExceeded.into()
            }
            _ => {
                ConnectorError::ProcessingStepFailed(Some(format!(
                    "Wave API error: {} - {}", status, error_message
                ).into()))
            }
        }
    } else {
        // Generic error for non-JSON responses
        ConnectorError::ProcessingStepFailed(Some(format!(
            "Wave API error {}: {}", status, body
        ).into()))
    }
}

#[derive(Debug, Deserialize)]
pub struct WaveAggregatedMerchantListResponse {
    pub aggregated_merchants: Vec<WaveAggregatedMerchant>,
    pub total_count: Option<i32>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveConnectorMetadata {
    pub aggregated_merchant_id: Option<String>,
    pub aggregated_merchant_name: Option<String>,
    pub auto_create_aggregated_merchant: Option<bool>,
    pub business_type: Option<WaveBusinessType>,
    pub business_description: Option<String>,
    pub manager_name: Option<String>,
    pub business_registration_identifier: Option<String>,
    pub business_sector: Option<String>,
    pub website_url: Option<String>,
    pub cache_enabled: Option<bool>,
    pub cache_ttl_seconds: Option<u64>,
}

impl Default for WaveConnectorMetadata {
    fn default() -> Self {
        Self {
            aggregated_merchant_id: None,
            aggregated_merchant_name: None,
            auto_create_aggregated_merchant: Some(false),
            business_type: Some(WaveBusinessType::default()),
            business_description: None,
            manager_name: None,
            business_registration_identifier: None,
            business_sector: None,
            website_url: None,
            cache_enabled: Some(true),
            cache_ttl_seconds: Some(3600), // 1 hour default
        }
    }
}


/// Extract aggregated merchant ID from router data connector metadata or business profile metadata
pub fn extract_aggregated_merchant_id(
    router_data: &PaymentsAuthorizeRouterData,
) -> Result<Option<String>, error_stack::Report<ConnectorError>> {
    // First try to get from connector account metadata
    if let Some(connector_meta) = &router_data.connector_meta_data {
        if let Ok(wave_metadata) = serde_json::from_value::<WaveConnectorMetadata>(connector_meta.peek().clone()) {
            if let Some(aggregated_merchant_id) = wave_metadata.aggregated_merchant_id {
                return Ok(Some(aggregated_merchant_id));
            }
        }
    }
    
    // If not found in connector metadata, try business profile metadata
    // This would require access to business profile data which might need to be passed separately
    // For now, return None to indicate no aggregated merchant configured
    Ok(None)
}

/// Extract Wave connector metadata from router data
pub fn extract_wave_connector_metadata(
    router_data: &PaymentsAuthorizeRouterData,
) -> Result<Option<WaveConnectorMetadata>, error_stack::Report<ConnectorError>> {
    if let Some(connector_meta) = &router_data.connector_meta_data {
        match serde_json::from_value::<WaveConnectorMetadata>(connector_meta.peek().clone()) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(_) => Ok(None), // Invalid metadata format, return None
        }
    } else {
        Ok(None)
    }
}

/// Build aggregated merchant request from business profile information with enhanced metadata support
pub fn build_aggregated_merchant_request_from_profile(
    profile_name: &str,
    metadata: Option<&WaveConnectorMetadata>,
) -> Result<WaveAggregatedMerchantRequest, WaveAggregatedMerchantError> {
    let default_description = format!("Payment processing for {}", profile_name);
    
    // Validate metadata if provided
    if let Some(meta) = metadata {
        validate_enhanced_wave_connector_metadata(meta, profile_name)?;
    }
    
    let request = WaveAggregatedMerchantRequest {
        name: profile_name.to_string(),
        business_type: metadata
            .and_then(|m| m.business_type.clone())
            .unwrap_or_default(),
        business_registration_identifier: metadata
            .and_then(|m| m.business_registration_identifier.clone()),
        business_sector: metadata
            .and_then(|m| m.business_sector.clone()),
        website_url: metadata
            .and_then(|m| m.website_url.clone()),
        business_description: metadata
            .and_then(|m| m.business_description.clone())
            .unwrap_or(default_description),
        manager_name: metadata.and_then(|m| m.manager_name.clone()),
    };
    
    // Validate the final request
    validate_wave_aggregated_merchant_request(&request)?;
    
    Ok(request)
}

/// Validate Wave connector metadata for aggregated merchants
pub fn validate_wave_connector_metadata(
    metadata: &WaveConnectorMetadata,
) -> Result<(), WaveAggregatedMerchantError> {
    // Validate aggregated merchant ID format if provided
    if let Some(ref merchant_id) = metadata.aggregated_merchant_id {
        if merchant_id.is_empty() {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Aggregated merchant ID cannot be empty".to_string(),
            });
        }
        
        // Check if ID follows Wave's format (am-xxxxxxxxx)
        if !merchant_id.starts_with("am-") || merchant_id.len() < 4 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Aggregated merchant ID must start with 'am-' and be properly formatted".to_string(),
            });
        }
    }
    
    // Validate business description length
    if let Some(ref description) = metadata.business_description {
        if description.len() > 500 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business description cannot exceed 500 characters".to_string(),
            });
        }
        
        if description.trim().is_empty() {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business description cannot be empty or only whitespace".to_string(),
            });
        }
    }
    
    // Validate manager name length
    if let Some(ref manager_name) = metadata.manager_name {
        if manager_name.len() > 100 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Manager name cannot exceed 100 characters".to_string(),
            });
        }
        
        if manager_name.trim().is_empty() {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Manager name cannot be empty or only whitespace".to_string(),
            });
        }
    }
    
    // Validate website URL format if provided
    if let Some(ref url) = metadata.website_url {
        if url.len() > 2083 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Website URL cannot exceed 2083 characters".to_string(),
            });
        }
        
        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Website URL must start with 'http://' or 'https://'".to_string(),
            });
        }
    }
    
    // Validate business registration identifier format if provided
    if let Some(ref identifier) = metadata.business_registration_identifier {
        if identifier.len() > 50 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business registration identifier cannot exceed 50 characters".to_string(),
            });
        }
    }
    
    // Validate business sector if provided
    if let Some(ref sector) = metadata.business_sector {
        if sector.len() > 100 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business sector cannot exceed 100 characters".to_string(),
            });
        }
    }
    
    // Validate auto-create configuration consistency
    if metadata.auto_create_aggregated_merchant == Some(true) {
        if metadata.aggregated_merchant_id.is_some() {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Cannot enable auto-create when aggregated merchant ID is already specified".to_string(),
            });
        }
        
        // For auto-creation, business description should be provided or derivable
        if metadata.business_description.is_none() {
            // This is not an error as we can generate a default description
            // but we could log a warning
        }
    }
    
    // Validate cache TTL if provided
    if let Some(cache_ttl) = metadata.cache_ttl_seconds {
        if cache_ttl < 60 || cache_ttl > 86400 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Cache TTL must be between 60 seconds and 24 hours".to_string(),
            });
        }
    }
    
    Ok(())
}

/// Enhanced validation for aggregated merchant metadata with business rules
pub fn validate_enhanced_wave_connector_metadata(
    metadata: &WaveConnectorMetadata,
    profile_name: &str,
) -> Result<(), WaveAggregatedMerchantError> {
    // First run basic validation
    validate_wave_connector_metadata(metadata)?;
    
    // Additional business rules validation
    if metadata.auto_create_aggregated_merchant == Some(true) {
        // For auto-creation, ensure we have sufficient information
        if metadata.business_type.is_none() {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business type is required for auto-creation of aggregated merchants".to_string(),
            });
        }
        
        // Validate profile name for auto-creation
        if profile_name.is_empty() || profile_name.len() > 255 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Profile name must be between 1 and 255 characters for auto-creation".to_string(),
            });
        }
    }
    
    Ok(())
}

/// Check if aggregated merchant configuration is ready for auto-creation
pub fn is_auto_creation_ready(metadata: &Option<WaveConnectorMetadata>) -> bool {
    match metadata {
        Some(meta) => {
            meta.auto_create_aggregated_merchant.unwrap_or(false) &&
            meta.aggregated_merchant_id.is_none() &&
            meta.business_type.is_some()
        }
        None => false,
    }
}

/// Get effective business description for aggregated merchant creation
pub fn get_effective_business_description(
    profile_name: &str,
    metadata: Option<&WaveConnectorMetadata>,
) -> String {
    metadata
        .and_then(|m| m.business_description.clone())
        .unwrap_or_else(|| format!("Payment processing for {}", profile_name))
}

/// Check if caching is enabled for aggregated merchant data
pub fn is_caching_enabled(metadata: &Option<WaveConnectorMetadata>) -> bool {
    metadata
        .as_ref()
        .and_then(|m| m.cache_enabled)
        .unwrap_or(true) // Default to enabled
}

/// Get cache TTL for aggregated merchant data
pub fn get_cache_ttl_seconds(metadata: &Option<WaveConnectorMetadata>) -> u64 {
    metadata
        .as_ref()
        .and_then(|m| m.cache_ttl_seconds)
        .unwrap_or(3600) // Default to 1 hour
}

/// Validate Wave aggregated merchant request before sending
pub fn validate_wave_aggregated_merchant_request(
    request: &WaveAggregatedMerchantRequest,
) -> Result<(), WaveAggregatedMerchantError> {
    // Validate merchant name
    if request.name.is_empty() || request.name.len() > 255 {
        return Err(WaveAggregatedMerchantError::InvalidConfiguration {
            details: "Merchant name must be between 1 and 255 characters".to_string(),
        });
    }
    
    // Validate business description
    if request.business_description.is_empty() {
        return Err(WaveAggregatedMerchantError::InvalidConfiguration {
            details: "Business description is required".to_string(),
        });
    }
    
    if request.business_description.len() > 500 {
        return Err(WaveAggregatedMerchantError::InvalidConfiguration {
            details: "Business description cannot exceed 500 characters".to_string(),
        });
    }
    
    // Validate website URL format if provided
    if let Some(ref url) = request.website_url {
        if url.len() > 2083 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Website URL cannot exceed 2083 characters".to_string(),
            });
        }
        
        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Website URL must start with 'http://' or 'https://'".to_string(),
            });
        }
    }
    
    // Validate business registration identifier format if provided
    if let Some(ref identifier) = request.business_registration_identifier {
        if identifier.len() > 50 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business registration identifier cannot exceed 50 characters".to_string(),
            });
        }
    }
    
    // Validate business sector if provided
    if let Some(ref sector) = request.business_sector {
        if sector.len() > 100 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Business sector cannot exceed 100 characters".to_string(),
            });
        }
    }
    
    // Validate manager name if provided
    if let Some(ref manager_name) = request.manager_name {
        if manager_name.len() > 100 {
            return Err(WaveAggregatedMerchantError::InvalidConfiguration {
                details: "Manager name cannot exceed 100 characters".to_string(),
            });
        }
    }
    
    Ok(())
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
        let status = AttemptStatus::from(item.response.status);
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

#[cfg(test)]
mod tests {
    use super::*;
    use common_enums::Currency;
    use common_utils::types::MinorUnit;
    use hyperswitch_domain_models::router_data::ConnectorAuthType;
    use masking::Secret;
    
    #[test]
    fn test_wave_auth_type_from_header_key() {
        let auth_type = ConnectorAuthType::HeaderKey {
            api_key: Secret::new("test_key".to_string()),
        };
        
        let wave_auth = WaveAuthType::try_from(&auth_type).unwrap();
        
        assert_eq!(wave_auth.api_key.peek(), "test_key");
        assert!(!wave_auth.aggregated_merchants_enabled);
        assert!(!wave_auth.auto_create_aggregated_merchant);
        assert_eq!(wave_auth.default_business_type, WaveBusinessType::Ecommerce);
        assert_eq!(wave_auth.cache_ttl_seconds, 3600);
    }
    
    #[test]
    fn test_wave_auth_type_from_body_key_with_config() {
        let config = WaveAggregatedMerchantConfig {
            enabled: true,
            auto_create_on_profile_creation: true,
            default_business_type: WaveBusinessType::Marketplace,
            cache_ttl_seconds: 7200,
        };
        
        let config_json = serde_json::to_string(&config).unwrap();
        
        let auth_type = ConnectorAuthType::BodyKey {
            api_key: Secret::new("test_key".to_string()),
            key1: Some(Secret::new(config_json)),
        };
        
        let wave_auth = WaveAuthType::try_from(&auth_type).unwrap();
        
        assert_eq!(wave_auth.api_key.peek(), "test_key");
        assert!(wave_auth.aggregated_merchants_enabled);
        assert!(wave_auth.auto_create_aggregated_merchant);
        assert_eq!(wave_auth.default_business_type, WaveBusinessType::Marketplace);
        assert_eq!(wave_auth.cache_ttl_seconds, 7200);
    }
    
    #[test]
    fn test_wave_business_type_default() {
        let business_type = WaveBusinessType::default();
        assert_eq!(business_type, WaveBusinessType::Ecommerce);
    }
    
    #[test]
    fn test_wave_business_type_serialization() {
        let business_type = WaveBusinessType::Marketplace;
        let serialized = serde_json::to_string(&business_type).unwrap();
        assert_eq!(serialized, "\"marketplace\"");
        
        let deserialized: WaveBusinessType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, WaveBusinessType::Marketplace);
    }
    
    #[test]
    fn test_wave_connector_metadata_validation_valid() {
        let metadata = WaveConnectorMetadata {
            aggregated_merchant_id: Some("am-test123".to_string()),
            aggregated_merchant_name: Some("Test Merchant".to_string()),
            auto_create_aggregated_merchant: Some(false),
            business_type: Some(WaveBusinessType::Ecommerce),
            business_description: Some("Test business".to_string()),
            manager_name: Some("John Doe".to_string()),
            business_registration_identifier: Some("REG123".to_string()),
            business_sector: Some("Technology".to_string()),
            website_url: Some("https://example.com".to_string()),
            cache_enabled: Some(true),
            cache_ttl_seconds: Some(3600),
        };
        
        let result = validate_wave_connector_metadata(&metadata);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_wave_connector_metadata_validation_invalid_merchant_id() {
        let metadata = WaveConnectorMetadata {
            aggregated_merchant_id: Some("invalid-id".to_string()),
            ..Default::default()
        };
        
        let result = validate_wave_connector_metadata(&metadata);
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        match error {
            WaveAggregatedMerchantError::InvalidConfiguration { details } => {
                assert!(details.contains("must start with 'am-'"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }
    
    #[test]
    fn test_is_auto_creation_ready() {
        // Test with valid auto-creation configuration
        let metadata = Some(WaveConnectorMetadata {
            auto_create_aggregated_merchant: Some(true),
            aggregated_merchant_id: None,
            business_type: Some(WaveBusinessType::Ecommerce),
            ..Default::default()
        });
        
        assert!(is_auto_creation_ready(&metadata));
        
        // Test with existing aggregated merchant ID
        let metadata_with_id = Some(WaveConnectorMetadata {
            auto_create_aggregated_merchant: Some(true),
            aggregated_merchant_id: Some("am-test123".to_string()),
            business_type: Some(WaveBusinessType::Ecommerce),
            ..Default::default()
        });
        
        assert!(!is_auto_creation_ready(&metadata_with_id));
    }
    
    #[test]
    fn test_get_effective_business_description() {
        let profile_name = "TestProfile";
        
        // Test with custom description
        let metadata = Some(WaveConnectorMetadata {
            business_description: Some("Custom business description".to_string()),
            ..Default::default()
        });
        
        let description = get_effective_business_description(profile_name, metadata.as_ref());
        assert_eq!(description, "Custom business description");
        
        // Test with default description
        let description = get_effective_business_description(profile_name, None);
        assert_eq!(description, "Payment processing for TestProfile");
    }
    
    #[test]
    fn test_validate_wave_aggregated_merchant_request_valid() {
        let request = WaveAggregatedMerchantRequest {
            name: "Test Merchant".to_string(),
            business_type: WaveBusinessType::Ecommerce,
            business_registration_identifier: Some("REG123".to_string()),
            business_sector: Some("Technology".to_string()),
            website_url: Some("https://example.com".to_string()),
            business_description: "Valid business description".to_string(),
            manager_name: Some("John Doe".to_string()),
        };
        
        let result = validate_wave_aggregated_merchant_request(&request);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_validate_wave_aggregated_merchant_request_invalid_name() {
        let request = WaveAggregatedMerchantRequest {
            name: "".to_string(), // Empty name
            business_type: WaveBusinessType::Ecommerce,
            business_registration_identifier: None,
            business_sector: None,
            website_url: None,
            business_description: "Valid business description".to_string(),
            manager_name: None,
        };
        
        let result = validate_wave_aggregated_merchant_request(&request);
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        match error {
            WaveAggregatedMerchantError::InvalidConfiguration { details } => {
                assert!(details.contains("Merchant name must be between"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }
    
    #[test]
    fn test_wave_aggregated_merchant_error_display() {
        let error = WaveAggregatedMerchantError::MerchantNotFound {
            merchant_id: "am-test123".to_string(),
        };
        
        let display = format!("{}", error);
        assert!(display.contains("Aggregated merchant not found: am-test123"));
    }
    
    #[test]
    fn test_parse_wave_api_error_aggregated_merchant_not_found() {
        let error_response = WaveErrorResponse {
            code: Some("AGGREGATED_MERCHANT_NOT_FOUND".to_string()),
            message: "Merchant not found".to_string(),
            details: None,
        };
        
        let body = serde_json::to_string(&error_response).unwrap();
        let connector_error = parse_wave_api_error(404, &body);
        
        // The error should be converted to a ProcessingStepFailed error
        match connector_error {
            ConnectorError::ProcessingStepFailed(_) => {}
            _ => panic!("Expected ProcessingStepFailed error"),
        }
    }
}
