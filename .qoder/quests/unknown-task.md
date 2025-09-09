# Wave Payment Connector Implementation Design

## Overview

This design document outlines the complete implementation of a fully functional Wave payment connector for Hyperswitch. The Wave connector will integrate with Wave's Business API to support payment processing, refunds, webhooks, and payment synchronization.

Wave is a mobile money and payment platform primarily focused on African markets, particularly Senegal (XOF), offering payment processing through mobile wallets and traditional payment methods.

## Technology Stack & Dependencies

### Wave API Information
- **Base URL**: `https://api.wave.com`
- **Authentication**: Bearer token using API keys
- **API Version**: v1 for most endpoints
- **Supported Currencies**: XOF (West African CFA franc), USD, EUR
- **Payment Methods**: Mobile money, bank transfers, cards
- **API Documentation**: https://docs.wave.com/business#api-reference

### Required Dependencies
- Wave API credentials (API key with appropriate scopes)
- Hyperswitch connector framework
- Serde for JSON serialization/deserialization
- Common utilities for error handling and authentication

## Architecture

### Connector Architecture Overview

The Wave connector follows Hyperswitch's standard connector architecture pattern:

```mermaid
graph TB
    A[Hyperswitch Router] --> B[Wave Connector]
    B --> C[Wave Transformers]
    C --> D[Wave API]
    
    B --> E[Payment Authorization]
    B --> F[Payment Capture]
    B --> G[Payment Sync]
    B --> H[Refund Processing]
    B --> I[Webhook Handling]
    
    E --> J[Checkout Sessions API]
    F --> K[Transaction Status API]
    G --> L[Payment Inquiry API]
    H --> M[Refund API]
    I --> N[Webhook Events]
```

### Core Components

#### 1. Wave Connector (wave.rs)
Main connector implementation containing:
- `ConnectorCommon` trait implementation
- Payment flow orchestration methods
- Error handling and response mapping
- URL construction and header management

#### 2. Wave Transformers (transformers.rs)
Data transformation layer containing:
- Request/response struct definitions
- Wave API data type mappings
- Hyperswitch-to-Wave data conversion logic
- Error response handling structures

#### 3. Authentication System
Wave uses Bearer token authentication:
- API keys obtained from Wave Business Portal
- Token-based authorization for all API calls
- Scope-based access control (checkout, balance, etc.)

## API Endpoints Reference

### Core Payment Endpoints

| Endpoint | Method | Purpose | Implementation |
|----------|--------|---------|----------------|
| `/v1/checkout/sessions` | POST | Create payment session | `PaymentAuthorize` |
| `/v1/checkout/sessions/{id}` | GET | Get session status | `PaymentSync` |
| `/v1/transactions/{id}` | GET | Get transaction details | `PaymentSync` |
| `/v1/transactions/{id}/refunds` | POST | Create refund | `RefundExecute` |
| `/v1/refunds/{id}` | GET | Get refund status | `RefundSync` |

### Request/Response Schema

#### Create Checkout Session Request
```json
{
  "amount": "1000",
  "currency": "XOF",
  "error_url": "https://merchant.com/error",
  "success_url": "https://merchant.com/success",
  "reference": "merchant_ref_123",
  "customer": {
    "name": "Customer Name",
    "email": "customer@example.com"
  }
}
```

#### Create Checkout Session Response
```json
{
  "id": "checkout_session_123",
  "launch_url": "https://checkout.wave.com/pay/123",
  "status": "created",
  "amount": "1000",
  "currency": "XOF",
  "reference": "merchant_ref_123"
}
```

### Authentication Requirements

All requests require Bearer token authentication:
```
Authorization: Bearer wave_sn_prod_YhUNb9d...i4bA6
```

## Data Models & Transformation Logic

### Authentication Structure
```rust
#[derive(Debug, Clone)]
pub struct WaveAuthType {
    pub api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for WaveAuthType {
    type Error = Report<ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::HeaderKey { api_key } => {
                Ok(Self {
                    api_key: api_key.to_owned(),
                })
            }
            _ => Err(ConnectorError::FailedToObtainAuthType)?,
        }
    }
}
```

### Payment Request Structures
```rust
#[derive(Debug, Serialize)]
pub struct WaveCheckoutSessionRequest {
    pub amount: String,
    pub currency: String,
    pub error_url: Option<String>,
    pub success_url: Option<String>,
    pub reference: Option<String>,
    pub customer: Option<WaveCustomer>,
}

#[derive(Debug, Serialize)]
pub struct WaveCustomer {
    pub name: Option<Secret<String>>,
    pub email: Option<Email>,
}
```

### Payment Response Structures
```rust
#[derive(Debug, Deserialize)]
pub struct WaveCheckoutSessionResponse {
    pub id: String,
    pub launch_url: Option<String>,
    pub status: WavePaymentStatus,
    pub amount: String,
    pub currency: String,
    pub reference: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WavePaymentStatus {
    Created,
    Pending,
    Completed,
    Failed,
    Cancelled,
}
```

### Status Mapping
```rust
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
```

## Business Logic Layer

### Payment Authorization Flow

```mermaid
sequenceDiagram
    participant M as Merchant
    participant H as Hyperswitch
    participant W as Wave Connector
    participant WA as Wave API
    
    M->>H: Create Payment
    H->>W: PaymentAuthorize Request
    W->>W: Transform Request Data
    W->>WA: POST /v1/checkout/sessions
    WA->>W: Session Created Response
    W->>W: Transform Response Data
    W->>H: PaymentResponse with redirect_url
    H->>M: Return payment session
    M->>WA: Redirect customer to launch_url
    WA->>M: Payment completion (webhook)
```

### Payment Capture
Wave uses automatic capture for checkout sessions. Manual capture is not supported in the current API version.

### Payment Synchronization Flow

```mermaid
sequenceDiagram
    participant H as Hyperswitch
    participant W as Wave Connector
    participant WA as Wave API
    
    H->>W: PaymentSync Request
    W->>WA: GET /v1/checkout/sessions/{id}
    WA->>W: Session Status Response
    W->>W: Map Status to Hyperswitch
    W->>H: Updated Payment Status
```

### Refund Processing Flow

```mermaid
sequenceDiagram
    participant H as Hyperswitch
    participant W as Wave Connector
    participant WA as Wave API
    
    H->>W: RefundExecute Request
    W->>WA: POST /v1/transactions/{id}/refunds
    WA->>W: Refund Created Response
    W->>W: Transform Response
    W->>H: Refund Status Response
```

## Error Handling Strategy

### Wave API Error Structure
```rust
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
```

### Error Status Code Mapping

| Wave Error | HTTP Status | Hyperswitch Error | Retry Strategy |
|------------|-------------|------------------|----------------|
| `invalid-auth` | 401 | `AuthenticationFailed` | No retry |
| `insufficient-funds` | 422 | `InsufficientFunds` | No retry |
| `invalid-amount` | 400 | `InvalidRequest` | No retry |
| `rate-limit-exceeded` | 429 | `RateLimited` | Exponential backoff |
| `internal-error` | 500 | `InternalServerError` | Linear retry |

### Error Response Transformation
```rust
impl From<WaveErrorResponse> for ErrorResponse {
    fn from(error: WaveErrorResponse) -> Self {
        ErrorResponse {
            code: error.code.unwrap_or_else(|| "unknown_error".to_string()),
            message: error.message,
            reason: error.details
                .and_then(|details| details.first().map(|d| d.msg.clone())),
            status_code: 0, // Set by HTTP layer
            attempt_status: None,
            connector_transaction_id: None,
        }
    }
}
```

## Webhook Handling Architecture

### Webhook Event Types
Wave sends webhooks for:
- `payment.completed` - Payment successfully processed
- `payment.failed` - Payment failed
- `payment.cancelled` - Payment cancelled by user
- `refund.completed` - Refund processed
- `refund.failed` - Refund failed

### Webhook Payload Structure
```rust
#[derive(Debug, Deserialize)]
pub struct WaveWebhookPayload {
    pub event_type: String,
    pub data: WaveWebhookData,
    pub timestamp: String,
    pub signature: String,
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
```

### Webhook Verification
```rust
pub fn verify_webhook_signature(
    payload: &str,
    signature: &str,
    secret: &str,
) -> Result<bool, ConnectorError> {
    // Implement HMAC-SHA256 signature verification
    // Wave uses header: X-Wave-Signature
    let expected_signature = generate_hmac_sha256(payload, secret)?;
    Ok(signature == expected_signature)
}
```

## Testing Strategy

### Unit Tests
- Authentication transformation tests
- Request/response transformation tests
- Error mapping tests
- Status conversion tests

### Integration Tests
- Payment authorization end-to-end
- Payment synchronization
- Refund processing
- Webhook handling
- Error scenario testing

### Test Configuration
Sample `auth.toml` configuration:
```toml
[wave]
api_key = "Bearer wave_sn_test_your_test_api_key_here"
webhook_secret = "your_webhook_secret_here"
```

### Test Data Requirements
- Valid Wave sandbox API credentials
- Test merchant account with appropriate permissions
- Webhook endpoint URL for testing
- Test amounts in supported currencies

## Configuration Examples

### Kubernetes Configuration
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: wave-connector-config
type: Opaque
stringData:
  api_key: "wave_sn_prod_your_production_api_key"
  webhook_secret: "your_webhook_secret"
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: wave-connector-urls
data:
  base_url: "https://api.wave.com"
  webhook_url: "https://your-domain.com/webhooks/wave"
```

### Helm Values
```yaml
connectors:
  wave:
    enabled: true
    baseUrl: "https://api.wave.com"
    credentials:
      apiKey:
        secretName: "wave-connector-config"
        secretKey: "api_key"
      webhookSecret:
        secretName: "wave-connector-config"
        secretKey: "webhook_secret"
```

### Environment Variables
```bash
# Production
WAVE_API_KEY="wave_sn_prod_your_production_api_key"
WAVE_WEBHOOK_SECRET="your_webhook_secret"
WAVE_BASE_URL="https://api.wave.com"

# Sandbox
WAVE_API_KEY="wave_sn_test_your_test_api_key"
WAVE_WEBHOOK_SECRET="your_test_webhook_secret"
WAVE_BASE_URL="https://api.wave.com"
```

## Implementation Roadmap

### Phase 1: Core Implementation
1. Complete `wave.rs` connector implementation
2. Implement authentication and authorization headers
3. Complete payment authorization flow
4. Implement basic error handling

### Phase 2: Payment Operations
1. Implement payment synchronization
2. Complete refund processing
3. Add comprehensive error mapping
4. Implement retry mechanisms

### Phase 3: Advanced Features
1. Webhook implementation and verification
2. Enhanced logging and observability
3. Performance optimizations
4. Comprehensive test coverage

### Phase 4: Production Readiness
1. Security review and hardening
2. Load testing and performance validation
3. Documentation completion
4. Deployment automation

## Security Considerations

### API Key Management
- Store API keys in secure secret management systems
- Use environment-specific keys (sandbox vs production)
- Implement key rotation procedures
- Monitor for unauthorized access attempts

### Webhook Security
- Implement signature verification for all webhooks
- Use HTTPS endpoints for webhook reception
- Validate all incoming webhook payloads
- Implement replay attack protection

### Data Protection
- Mask sensitive payment information in logs
- Encrypt API communications using TLS 1.2+
- Implement proper error message sanitization
- Follow PCI DSS guidelines for payment data handling

### Rate Limiting
- Implement exponential backoff for retries
- Respect Wave's rate limiting policies
- Monitor API usage patterns
- Implement circuit breaker patterns for resilience

## Monitoring & Observability

### Key Metrics
- Payment success rates by currency and amount
- API response times and latency percentiles
- Error rates by error type and HTTP status
- Webhook delivery success rates

### Logging Strategy
- Structured logging with correlation IDs
- Sensitive data masking in log outputs
- Request/response logging for debugging
- Performance metrics collection

### Alerting
- High error rate alerts
- API latency degradation alerts
- Authentication failure alerts
- Webhook delivery failure alerts

## Compliance & Regulatory

### Wave API Compliance
- Adhere to Wave's API usage policies
- Implement proper error handling per Wave guidelines
- Follow Wave's webhook security requirements
- Maintain compliance with Wave's data retention policies

### Financial Regulations
- Support for transaction reporting requirements
- Implement anti-money laundering (AML) checks
- Maintain audit trails for all transactions
- Support regulatory reporting formats

## Performance Optimization

### Caching Strategy
- Cache authentication tokens with appropriate TTL
- Implement response caching for idempotent operations
- Use connection pooling for HTTP clients
- Optimize serialization/deserialization performance

### Concurrency
- Support parallel processing of multiple payments
- Implement async/await patterns for I/O operations
- Use non-blocking HTTP clients
- Optimize thread pool configurations

### Resource Management
- Implement proper connection lifecycle management
- Monitor memory usage patterns
- Optimize JSON parsing performance
- Use efficient data structures for transformations