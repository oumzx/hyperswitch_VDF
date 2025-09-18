# Wave Connector Implementation Guide

## Overview

The Wave connector integrates Hyperswitch with Wave's payment processing platform, enabling secure payment transactions using the XOF (West African Franc) currency. This implementation includes comprehensive support for Wave's Aggregated Merchants API, allowing for flexible business configurations and enhanced payment processing capabilities.

## Features

### Core Payment Operations
- **Payment Authorization**: Create checkout sessions for customer payments
- **Payment Sync**: Real-time payment status synchronization
- **Payment Void**: Cancel pending payment sessions
- **Refund Processing**: Full and partial refund support
- **Webhook Support**: Real-time payment status notifications (structure defined)

### Advanced Features
- **Aggregated Merchants**: Support for Wave's multi-merchant functionality
- **Auto-Creation**: Automatic aggregated merchant creation based on business profiles
- **XOF Currency**: Exclusive support for West African Franc with proper formatting
- **Mobile Payment Redirect**: Native support for Wave app-based payments

## API Compatibility

### Wave API Endpoints
- **Checkout Sessions**: `/v1/checkout/sessions` - Create and manage payment sessions
- **Session Status**: `/v1/checkout/sessions/{id}` - Query payment status
- **Session Expiry**: `/v1/checkout/sessions/{id}/expire` - Cancel sessions
- **Refunds**: `/v1/checkout/sessions/{id}/refund` - Process refunds
- **Aggregated Merchants**: `/v1/aggregated_merchants` - Manage merchant identities

### Supported Operations
| Operation | Status | Wave Endpoint | Notes |
|-----------|--------|---------------|--------|
| Authorization | ✅ | POST /v1/checkout/sessions | Creates payment session |
| Sync | ✅ | GET /v1/checkout/sessions/{id} | Real-time status |
| Void | ✅ | POST /v1/checkout/sessions/{id}/expire | Cancel session |
| Capture | ❌ | N/A | Wave uses automatic capture |
| Refund | ✅ | POST /v1/checkout/sessions/{id}/refund | Full/partial refunds |
| Refund Sync | ✅ | GET /v1/refunds/{id} | Refund status tracking |

## Configuration

### Basic Setup

```json
{
  "connector_name": "wave",
  "auth_type": "HeaderKey",
  "api_key": "sk_test_your_wave_api_key_here"
}
```

### Enhanced Setup with Aggregated Merchants

```json
{
  "connector_name": "wave",
  "auth_type": "BodyKey",
  "api_key": "sk_test_your_wave_api_key_here",
  "key1": "aggregated_merchants_enabled"
}
```

### Environment Configuration

Update your configuration files with Wave endpoints:

```toml
# config/development.toml
[connectors.wave]
base_url = "https://api.sandbox.wave.com/"

# config/production.toml
[connectors.wave]
base_url = "https://api.wave.com/"
```

## Payment Integration

### Basic Payment Flow

1. **Create Payment**
```rust
// Payment request with XOF currency
let payment_data = PaymentsAuthorizeData {
    currency: Currency::XOF,
    amount: 1000, // 1000 XOF
    payment_method_data: PaymentMethodData::Wallet(
        WalletData::MobilePayRedirect(
            Box::new(MobilePayRedirection {})
        )
    ),
    email: Some(Email::from_str("customer@example.com")?),
    customer_name: Some(Secret::new("Customer Name".to_string())),
    router_return_url: Some("https://merchant.com/return".to_string()),
    // ... other fields
};
```

2. **Handle Response**
```rust
// Expected response structure
match response.response? {
    PaymentsResponseData::TransactionResponse {
        resource_id: ResponseId::ConnectorTransactionId(session_id),
        redirection_data,
        ..
    } => {
        // session_id: Wave checkout session ID
        // redirection_data: Contains wave_launch_url for mobile payment
    }
}
```

### Payment Status Tracking

```rust
// Sync payment status
let sync_data = PaymentsSyncData {
    connector_transaction_id: ResponseId::ConnectorTransactionId(session_id),
    ..Default::default()
};

// Handle status updates
match sync_response.status {
    AttemptStatus::Pending => // Payment awaiting completion
    AttemptStatus::Charged => // Payment successful
    AttemptStatus::Failure => // Payment failed
    AttemptStatus::Voided => // Payment cancelled
}
```

## Aggregated Merchants Integration

### Configuration

Add aggregated merchant metadata to your business profile:

```json
{
  "wave_aggregated_merchant": {
    "aggregated_merchant_id": "am-7lks22ap113t4",
    "auto_create": true,
    "business_type": "fintech",
    "business_description": "Payment processing for e-commerce",
    "manager_name": "Business Manager"
  }
}
```

### Auto-Creation Setup

```json
{
  "wave_aggregated_merchant": {
    "auto_create_aggregated_merchant": true,
    "business_type": "other",
    "business_description": "Custom business description",
    "manager_name": "Manager Name"
  }
}
```

### Metadata Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `aggregated_merchant_id` | String | No | Direct mapping to Wave aggregated merchant |
| `auto_create_aggregated_merchant` | Boolean | No | Enable automatic creation |
| `business_type` | Enum | No | "fintech" or "other" |
| `business_description` | String | No* | Description for aggregated merchant |
| `manager_name` | String | No | Manager contact name |

*Required for auto-creation if not provided, defaults to "Payment processing for {profile_name}"

## Currency Support

### XOF (West African Franc)
- **Only Supported Currency**: Wave exclusively processes XOF transactions
- **No Decimal Places**: Amounts must be whole numbers (e.g., 1000, not 10.00)
- **Minor Unit Ratio**: 1:1 (1 XOF = 1 minor unit)

### Amount Formatting
```rust
// Correct: Whole number amounts
amount: 1000  // 1000 XOF

// Incorrect: Decimal amounts will cause errors
amount: 10.50 // Not supported
```

## Error Handling

### Common Error Scenarios

1. **Invalid Currency**
```json
{
  "error": {
    "code": "INVALID_CURRENCY",
    "message": "Currency USD not supported. Only XOF is accepted."
  }
}
```

2. **Invalid Amount**
```json
{
  "error": {
    "code": "INVALID_AMOUNT",
    "message": "Amount must be a positive integer for XOF currency."
  }
}
```

3. **Aggregated Merchant Not Found**
```json
{
  "error": {
    "code": "AGGREGATED_MERCHANT_NOT_FOUND",
    "message": "Aggregated merchant am-invalid123 not found."
  }
}
```

### Error Recovery

```rust
// Graceful error handling for aggregated merchants
match aggregated_merchant_result {
    Ok(merchant_id) => {
        // Use aggregated merchant for payment
    }
    Err(_) => {
        // Log warning and continue with standard processing
        log::warn!("Aggregated merchant unavailable, using default processing");
        // Payment continues without aggregated merchant context
    }
}
```

## Testing

### Test Configuration

Create test authentication file:

```toml
# crates/router/tests/connectors/wave_test_auth.toml
[wave]
api_key = "sk_test_your_sandbox_key_here"

[wave.sandbox]
api_key = "sk_test_sandbox_key_here"
base_url = "https://api.sandbox.wave.com/"
```

### Sample Test Cases

```rust
#[actix_web::test]
async fn test_wave_xof_payment() {
    let connector = Wave {};
    let payment_data = get_default_xof_payment_data();
    
    let response = connector
        .authorize_payment(payment_data, None)
        .await
        .unwrap();
    
    assert_eq!(response.status, AttemptStatus::Pending);
    assert!(response.response.is_ok());
}

#[actix_web::test]
async fn test_currency_validation() {
    let connector = Wave {};
    let usd_payment = get_payment_data_with_currency(Currency::USD);
    
    let response = connector
        .authorize_payment(usd_payment, None)
        .await;
    
    assert!(response.is_err()); // USD should be rejected
}
```

### Running Tests

```bash
# Run Wave connector tests
cargo test --package router --test connectors wave

# Run specific test
cargo test --package router --test connectors should_only_authorize_payment
```

## Monitoring & Observability

### Key Metrics

- `wave_aggregated_merchant_cache_hit_rate`: Aggregated merchant cache efficiency
- `wave_aggregated_merchant_api_errors`: API failure rate
- `wave_payments_with_aggregated_merchant`: Feature adoption rate
- `wave_checkout_session_creation_time`: Performance monitoring

### Logging

```rust
// Structured logging examples
log::info!(
    "Wave checkout session created",
    session_id = %session_id,
    amount = %amount,
    currency = "XOF",
    aggregated_merchant_id = %aggregated_merchant_id.as_deref().unwrap_or("none")
);

log::warn!(
    "Aggregated merchant auto-creation failed",
    profile_id = %profile_id,
    error = %error,
    fallback_used = true
);
```

## Security Considerations

### API Key Management
- Use environment variables for API keys
- Implement key rotation procedures
- Monitor for unauthorized API usage
- Separate sandbox and production credentials

### Data Protection
- PII fields are properly masked in logs
- Aggregated merchant IDs are encrypted in storage
- Sensitive data follows Hyperswitch masking patterns

### Rate Limiting
- Wave API has rate limits that the connector respects
- Implements exponential backoff for failed requests
- Caches aggregated merchant data to reduce API calls

## Migration Guide

### From Basic to Aggregated Merchants

1. **Update Authentication**
```json
// Before
{
  "auth_type": "HeaderKey",
  "api_key": "sk_test_key"
}

// After
{
  "auth_type": "BodyKey",
  "api_key": "sk_test_key",
  "key1": "aggregated_merchants_enabled"
}
```

2. **Add Business Profile Metadata**
```json
{
  "wave_aggregated_merchant": {
    "auto_create_aggregated_merchant": true,
    "business_type": "other",
    "business_description": "Your business description"
  }
}
```

3. **Backward Compatibility**
- Existing configurations continue to work
- Aggregated merchant features are opt-in
- No breaking changes to payment flows

## Troubleshooting

### Common Issues

1. **Payment Stuck in Pending**
   - Check Wave app completion status
   - Verify customer received redirect URL
   - Use sync endpoint to get latest status

2. **Aggregated Merchant Creation Failed**
   - Verify API key permissions
   - Check business description length (<500 chars)
   - Ensure auto_create is properly configured

3. **Currency Errors**
   - Ensure only XOF currency is used
   - Verify amounts are integers (no decimals)
   - Check amount is positive

### Debug Mode

Enable detailed logging:

```rust
RUST_LOG=hyperswitch_connectors::connectors::wave=debug cargo run
```

### Support

- **Hyperswitch Documentation**: [docs.hyperswitch.io](https://docs.hyperswitch.io)
- **Wave API Documentation**: [docs.wave.com](https://docs.wave.com)
- **Community Support**: [Hyperswitch Slack](https://inviter.co/hyperswitch-slack)

## API Reference

### Data Structures

#### WaveCheckoutSessionRequest
```rust
pub struct WaveCheckoutSessionRequest {
    pub amount: String,                    // "1000"
    pub currency: String,                  // "XOF"
    pub error_url: Option<String>,         // Return URL on error
    pub success_url: Option<String>,       // Return URL on success
    pub reference: Option<String>,         // Merchant reference
    pub aggregated_merchant_id: Option<String>, // Aggregated merchant ID
    pub customer: Option<WaveCustomer>,    // Customer information
}
```

#### WaveCheckoutSessionResponse
```rust
pub struct WaveCheckoutSessionResponse {
    pub id: String,                        // "cos-18qq25rgr100a"
    pub launch_url: Option<String>,        // Wave app redirect URL
    pub status: WavePaymentStatus,         // Payment status
    pub amount: String,                    // "1000"
    pub currency: String,                  // "XOF"
    pub reference: Option<String>,         // Merchant reference
}
```

#### WaveAggregatedMerchant
```rust
pub struct WaveAggregatedMerchant {
    pub id: String,                        // "am-7lks22ap113t4"
    pub name: String,                      // Merchant name
    pub business_type: WaveBusinessType,   // fintech | other
    pub business_description: String,      // Business description
    pub is_locked: bool,                   // Merchant status
    pub when_created: String,              // ISO 8601 timestamp
    // ... other fields
}
```

### Status Mappings

#### Payment Status
| Wave Status | Hyperswitch Status | Description |
|-------------|-------------------|-------------|
| `created` | `Pending` | Session created, awaiting payment |
| `pending` | `Pending` | Payment in progress |
| `completed` | `Charged` | Payment successful |
| `failed` | `Failure` | Payment failed |
| `cancelled` | `Voided` | Payment cancelled |

#### Refund Status
| Wave Status | Hyperswitch Status | Description |
|-------------|-------------------|-------------|
| `processing` | `Pending` | Refund being processed |
| `completed` | `Success` | Refund successful |
| `failed` | `Failure` | Refund failed |
| `cancelled` | `Failure` | Refund cancelled |

## Conclusion

The Wave connector provides comprehensive payment processing capabilities for the West African market with robust support for XOF currency transactions. The aggregated merchants integration adds flexibility for businesses requiring multiple merchant identities while maintaining backward compatibility with existing implementations.

For production deployment, ensure proper API key management, monitoring setup, and thorough testing with Wave's sandbox environment before processing live transactions.