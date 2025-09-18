# Wave API Testing with XOF Currency Support - Design Document

## Overview

This document outlines the design for comprehensive testing of the Wave connector in Hyperswitch with a specific focus on XOF (West African Franc) currency support. The design addresses the current test failures and provides a robust testing framework aligned with Wave's API specifications.

## Wave API Analysis

### Supported Currencies
- **Primary Currency**: XOF (West African Franc)
- **Currency Constraints**: No decimal places allowed for XOF amounts
- **Amount Format**: String representation without decimals for XOF

### Authentication
- **Method**: Bearer Token Authentication
- **Header**: `Authorization: Bearer wave_sn_prod_YhUNb9d...`
- **Key Management**: API keys managed through Wave Business Portal

### Core API Endpoints

```mermaid
graph TD
    A[Checkout API] --> B[POST /v1/checkout/sessions]
    A --> C[GET /v1/checkout/sessions/:id]
    A --> D[POST /v1/checkout/sessions/:id/expire]
    A --> E[POST /v1/checkout/sessions/:id/refund]
    
    F[Aggregated Merchants API] --> G[GET /v1/aggregated_merchants]
    F --> H[POST /v1/aggregated_merchants]
    F --> I[GET /v1/aggregated_merchants/:id]
```

## Current Test Failures Analysis

### Root Cause Analysis

| Test Case | Error Pattern | Root Cause |
|-----------|---------------|------------|
| `should_authorize_payment_with_usd` | `Unexpected state reached/Invariants conflicted` | USD currency not supported by Wave (XOF only) |
| `should_refund_succeeded_payment` | Same error pattern | Authentication or API endpoint mismatch |
| `should_sync_authorized_payment` | Same error pattern | Incorrect session ID handling |
| `should_void_authorized_payment` | Same error pattern | Missing void/cancel implementation |
| `wave_attach_connector_and_check` | `missing field merchant_id` | Connector registration data structure issue |

### Technical Issues Identified

1. **Currency Validation**: Tests using USD currency when Wave only supports XOF
2. **Authentication Configuration**: Potential mismatch in auth token format
3. **API Endpoint Structure**: Incorrect URL construction for Wave API
4. **Response Parsing**: Mismatch between expected and actual response formats
5. **Connector Registration**: Missing required fields in connector setup

## Revised Test Architecture

### Test Structure

```mermaid
flowchart TB
    A[Wave Test Suite] --> B[Authentication Tests]
    A --> C[Payment Flow Tests]
    A --> D[Currency Validation Tests]
    A --> E[Error Handling Tests]
    A --> F[Integration Tests]
    
    B --> B1[Valid API Key]
    B --> B2[Invalid API Key]
    B --> B3[Missing Auth Header]
    
    C --> C1[Create Checkout Session]
    C --> C2[Payment Status Sync]
    C --> C3[Payment Expiration]
    C --> C4[Refund Processing]
    
    D --> D1[XOF Currency Validation]
    D --> D2[Invalid Currency Rejection]
    D --> D3[Amount Format Validation]
    
    E --> E1[Network Errors]
    E --> E2[API Rate Limiting]
    E --> E3[Invalid Request Data]
    
    F --> F1[Connector Registration]
    F --> F2[Health Check]
    F --> F3[End-to-End Flow]
```

### XOF Currency Test Specifications

#### Amount Handling
- **Format**: String without decimal places
- **Validation**: Positive integer values only
- **Examples**:
  - Valid: "1000" (represents 1000 XOF)
  - Invalid: "10.50", "0", "-100"

#### Currency Constraints
```rust
// XOF specific validation
fn validate_xof_amount(amount: &str) -> Result<(), ValidationError> {
    // No decimal places allowed
    if amount.contains('.') {
        return Err(ValidationError::DecimalNotAllowed);
    }
    
    // Must be positive
    let parsed: i64 = amount.parse()?;
    if parsed <= 0 {
        return Err(ValidationError::InvalidAmount);
    }
    
    Ok(())
}
```

## Test Implementation Design

### Core Test Functions

#### 1. Authentication Test Cases
```rust
#[actix_web::test]
async fn test_wave_authentication_valid_key() {
    // Test with valid Wave API key
    // Expected: Successful connection
}

#[actix_web::test]
async fn test_wave_authentication_invalid_key() {
    // Test with invalid API key
    // Expected: 401 Unauthorized error
}
```

#### 2. XOF Currency Payment Tests
```rust
#[actix_web::test]
async fn should_create_checkout_session_xof() {
    let payment_data = PaymentsAuthorizeData {
        currency: Currency::XOF,
        minor_amount: MinorUnit::new(1000), // 1000 XOF
        // ... other required fields
    };
    
    let response = Wave::new()
        .authorize_payment(Some(payment_data), None)
        .await
        .unwrap();
    
    assert_eq!(response.status, AttemptStatus::Pending);
    // Verify checkout session URL is returned
}
```

#### 3. Payment Status Synchronization
```rust
#[actix_web::test]
async fn should_sync_checkout_session_status() {
    // Create checkout session first
    let checkout_response = create_test_checkout_session().await;
    let session_id = extract_session_id(checkout_response);
    
    // Sync payment status
    let sync_response = Wave::new()
        .psync_retry_till_status_matches(
            AttemptStatus::Pending,
            session_id,
            None,
        )
        .await
        .unwrap();
    
    // Verify status is one of: open, complete, expired
}
```

#### 4. Refund Processing Tests
```rust
#[actix_web::test]
async fn should_process_refund_for_completed_payment() {
    // Create and complete payment first
    let payment_response = create_and_complete_payment().await;
    let transaction_id = extract_transaction_id(payment_response);
    
    // Process refund
    let refund_response = Wave::new()
        .refund_payment(
            transaction_id,
            RefundsData {
                refund_amount: 500, // Partial refund
                currency: Currency::XOF,
                // ... other fields
            },
            None,
        )
        .await
        .unwrap();
    
    assert!(matches!(
        refund_response.refund_status,
        RefundStatus::Pending | RefundStatus::Success
    ));
}
```

### Error Handling Test Cases

#### Currency Validation Tests
```rust
#[actix_web::test]
async fn should_reject_unsupported_currency() {
    let invalid_currencies = vec![
        Currency::USD,
        Currency::EUR,
        Currency::GBP,
        Currency::JPY,
    ];
    
    for currency in invalid_currencies {
        let result = Wave::new()
            .authorize_payment(
                PaymentsAuthorizeData {
                    currency,
                    // ... other fields
                },
                None,
            )
            .await;
        
        assert!(result.is_err());
        // Verify specific error code for unsupported currency
    }
}
```

#### Amount Validation Tests
```rust
#[actix_web::test]
async fn should_reject_invalid_amounts() {
    let invalid_amounts = vec![0, -100, -1];
    
    for amount in invalid_amounts {
        let result = Wave::new()
            .authorize_payment(
                PaymentsAuthorizeData {
                    currency: Currency::XOF,
                    minor_amount: MinorUnit::new(amount),
                    // ... other fields
                },
                None,
            )
            .await;
        
        assert!(result.is_err());
    }
}
```

## Test Configuration

### Environment Setup
```toml
# test configuration for Wave connector
[wave]
api_key = "${WAVE_API_KEY}"  # Set via environment variable
base_url = "https://api.wave.com"
supported_currencies = ["XOF"]
default_currency = "XOF"
timeout_seconds = 30
```

### Test Data Templates

#### Checkout Session Request
```json
{
    "amount": "1000",
    "currency": "XOF",
    "success_url": "https://example.com/success",
    "error_url": "https://example.com/error",
    "client_reference": "test_ref_12345"
}
```

#### Expected Response Structure
```json
{
    "id": "cos-18qq25rgr100a",
    "amount": "1000",
    "currency": "XOF",
    "checkout_status": "open",
    "payment_status": "processing",
    "wave_launch_url": "https://pay.wave.com/c/cos-18qq25rgr100a",
    "when_created": "2024-01-15T10:13:04Z",
    "when_expires": "2024-01-15T10:43:04Z"
}
```

## Integration Test Flows

### End-to-End Payment Flow
```mermaid
sequenceDiagram
    participant T as Test Suite
    participant H as Hyperswitch
    participant W as Wave API
    
    T->>H: Create Checkout Session
    H->>W: POST /v1/checkout/sessions
    W-->>H: Session Details + Launch URL
    H-->>T: Payment Response with Redirection
    
    T->>H: Sync Payment Status
    H->>W: GET /v1/checkout/sessions/{id}
    W-->>H: Session Status
    H-->>T: Payment Status
    
    Note over T,W: Payment completion happens outside test
    
    T->>H: Process Refund
    H->>W: POST /v1/checkout/sessions/{id}/refund
    W-->>H: Refund Details
    H-->>T: Refund Response
```

### Connector Registration Flow
```mermaid
sequenceDiagram
    participant T as Test Suite
    participant A as Admin API
    participant H as Hyperswitch Core
    
    T->>A: Create Merchant Account
    A-->>T: Merchant ID
    
    T->>A: Register Wave Connector
    Note right of T: Include API key and configuration
    A->>H: Validate Connector Config
    H-->>A: Validation Result
    A-->>T: Connector Registration Success
    
    T->>A: Test Connector Health
    A->>H: Health Check Request
    H-->>A: Connector Status
    A-->>T: Health Status
```

## Test Execution Strategy

### Test Categories

1. **Unit Tests**: Individual component validation
2. **Integration Tests**: API interaction validation
3. **End-to-End Tests**: Complete payment flow validation
4. **Error Handling Tests**: Edge case and error condition handling

### Test Environment Requirements

- **Wave Sandbox Account**: For API testing
- **Valid API Keys**: For authentication testing
- **Network Access**: To Wave API endpoints
- **Test Data**: Predefined test scenarios

### Performance Benchmarks

| Test Category | Expected Duration | Success Criteria |
|---------------|------------------|------------------|
| Authentication Tests | < 5 seconds | 100% pass rate |
| Currency Validation | < 2 seconds | Proper error handling |
| Payment Creation | < 10 seconds | Session created successfully |
| Status Synchronization | < 15 seconds | Accurate status retrieval |
| Refund Processing | < 20 seconds | Refund initiated successfully |

## Quality Assurance

### Test Coverage Targets
- **Code Coverage**: Minimum 85%
- **API Endpoint Coverage**: 100% of implemented endpoints
- **Error Scenario Coverage**: All documented error codes
- **Currency Support**: Complete XOF validation

### Automated Testing Pipeline
- **Pre-commit Hooks**: Run unit tests
- **CI/CD Integration**: Full test suite execution
- **Nightly Regression**: End-to-end flow validation
- **Performance Monitoring**: Response time tracking