# Wave Aggregated Merchants Integration Guide

## Overview

This guide describes the integration of Wave's Aggregated Merchants API with the Hyperswitch Wave connector. This enhancement allows Hyperswitch merchants to utilize Wave's aggregated merchant identities for payment processing, enabling them to operate under different merchant names and fee structures while maintaining backward compatibility.

## Table of Contents

1. [Features](#features)
2. [Configuration](#configuration)
3. [Business Profile Integration](#business-profile-integration)
4. [API Reference](#api-reference)
5. [Error Handling](#error-handling)
6. [Testing](#testing)
7. [Migration Guide](#migration-guide)
8. [Troubleshooting](#troubleshooting)

## Features

### Core Capabilities

- **Multiple Merchant Identities**: Operate under different business names through a single Hyperswitch merchant account
- **Flexible Fee Structures**: Utilize different fee structures for different business contexts
- **Auto-Creation**: Automatically create aggregated merchants based on business profile information
- **Caching**: Intelligent caching of aggregated merchant data for improved performance
- **Backward Compatibility**: Existing Wave connector functionality remains unchanged

### Supported Operations

- Create, read, update, and delete aggregated merchants
- Automatic resolution of aggregated merchants for payments
- Enhanced error handling and validation
- Comprehensive logging and monitoring

## Configuration

### Enhanced Authentication

The Wave connector supports enhanced authentication configuration for aggregated merchants:

#### Standard Configuration (Header Key)

```toml
[wave]
api_key = "sk_test_your_wave_api_key_here"
```

#### Enhanced Configuration (Body Key with Aggregated Merchants)

```toml
[wave.enhanced]
api_key = "sk_test_enhanced_wave_key_here"
key1 = '''
{
  "enabled": true,
  "auto_create_on_profile_creation": false,
  "default_business_type": "ecommerce",
  "cache_ttl_seconds": 3600
}
'''
```

### Configuration Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable aggregated merchants feature |
| `auto_create_on_profile_creation` | boolean | `false` | Auto-create aggregated merchants for new profiles |
| `default_business_type` | string | `"ecommerce"` | Default business type for auto-created merchants |
| `cache_ttl_seconds` | number | `3600` | Cache TTL in seconds (60-86400) |

### Business Types

Supported business types for aggregated merchants:

- `ecommerce` - E-commerce businesses
- `mobile` - Mobile applications
- `pos` - Point of sale systems
- `marketplace` - Online marketplaces
- `subscription` - Subscription services
- `other` - Other business types

## Business Profile Integration

### Connector Metadata Schema

Business profiles can include Wave-specific metadata for aggregated merchant configuration:

```json
{
  "wave_aggregated_merchant": {
    "aggregated_merchant_id": "am-7lks22ap113t4",
    "aggregated_merchant_name": "My Business Name",
    "auto_create": true,
    "business_type": "marketplace",
    "business_description": "Online marketplace for artisans",
    "manager_name": "John Doe",
    "business_registration_identifier": "REG123456",
    "business_sector": "Technology",
    "website_url": "https://mybusiness.com",
    "cache_enabled": true,
    "cache_ttl_seconds": 7200
  }
}
```

### Metadata Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `aggregated_merchant_id` | string | No | Existing Wave aggregated merchant ID |
| `aggregated_merchant_name` | string | No | Display name for the merchant |
| `auto_create` | boolean | No | Enable auto-creation for this profile |
| `business_type` | enum | No | Business type classification |
| `business_description` | string | No | Business description (max 500 chars) |
| `manager_name` | string | No | Manager contact name (max 100 chars) |
| `business_registration_identifier` | string | No | Business registration number (max 50 chars) |
| `business_sector` | string | No | Business sector (max 100 chars) |
| `website_url` | string | No | Business website URL (max 2083 chars) |
| `cache_enabled` | boolean | No | Enable caching for this profile |
| `cache_ttl_seconds` | number | No | Cache TTL override (60-86400) |

### Resolution Flow

1. **Profile Lookup**: Extract aggregated merchant metadata from business profile
2. **Direct Mapping**: If `aggregated_merchant_id` is specified, use it directly
3. **Validation**: Validate that the aggregated merchant exists and is accessible
4. **Auto-Creation**: If enabled and no merchant ID exists, create a new aggregated merchant
5. **Fallback**: If resolution fails, proceed with standard payment processing

## API Reference

### Wave Aggregated Merchants API

#### List Aggregated Merchants

```http
GET /v1/aggregated_merchants
Authorization: Bearer {api_key}
```

Query Parameters:
- `limit` (optional): Maximum number of merchants to return
- `cursor` (optional): Pagination cursor

#### Create Aggregated Merchant

```http
POST /v1/aggregated_merchants
Authorization: Bearer {api_key}
Content-Type: application/json

{
  "name": "My Business",
  "business_type": "ecommerce",
  "business_description": "Online retail business",
  "business_registration_identifier": "REG123456",
  "business_sector": "Retail",
  "website_url": "https://mybusiness.com",
  "manager_name": "John Doe"
}
```

#### Get Aggregated Merchant

```http
GET /v1/aggregated_merchants/{id}
Authorization: Bearer {api_key}
```

#### Update Aggregated Merchant

```http
PUT /v1/aggregated_merchants/{id}
Authorization: Bearer {api_key}
Content-Type: application/json

{
  "name": "Updated Business Name",
  "business_description": "Updated description"
}
```

#### Delete Aggregated Merchant

```http
DELETE /v1/aggregated_merchants/{id}
Authorization: Bearer {api_key}
```

### Enhanced Checkout Session

When aggregated merchants are configured, checkout sessions automatically include the aggregated merchant context:

```json
{
  "amount": "1000",
  "currency": "XOF",
  "success_url": "https://merchant.com/success",
  "error_url": "https://merchant.com/error",
  "reference": "payment-123",
  "aggregated_merchant_id": "am-7lks22ap113t4",
  "customer": {
    "name": "Customer Name",
    "email": "customer@example.com"
  }
}
```

## Error Handling

### Aggregated Merchant Errors

| Error Code | HTTP Status | Description | Resolution |
|------------|-------------|-------------|------------|
| `AGGREGATED_MERCHANT_NOT_FOUND` | 404 | Specified merchant not found | Verify merchant ID or enable auto-creation |
| `INVALID_BUSINESS_TYPE` | 400 | Invalid business type specified | Use supported business type |
| `INVALID_CONFIGURATION` | 400 | Invalid metadata configuration | Check field validation rules |
| `AUTO_CREATION_DISABLED` | 400 | Auto-creation not enabled | Enable auto-creation or provide merchant ID |
| `RATE_LIMIT_EXCEEDED` | 429 | API rate limit exceeded | Implement exponential backoff |
| `AUTHENTICATION_FAILED` | 401/403 | Invalid API credentials | Verify API key permissions |

### Graceful Degradation

The connector implements graceful degradation strategies:

1. **Configuration Errors**: Log warnings and proceed with standard processing
2. **API Failures**: Retry with exponential backoff, fallback to standard processing
3. **Validation Errors**: Provide detailed error messages for configuration issues
4. **Rate Limiting**: Implement automatic retry with appropriate delays

### Error Response Format

```json
{
  "code": "AGGREGATED_MERCHANT_NOT_FOUND",
  "message": "Aggregated merchant am-invalid123 not found",
  "details": [
    {
      "loc": ["aggregated_merchant_id"],
      "msg": "Merchant ID must start with 'am-' and be properly formatted"
    }
  ]
}
```

## Testing

### Unit Tests

The implementation includes comprehensive unit tests covering:

- Configuration validation
- Aggregated merchant resolution
- Error handling scenarios
- API request/response transformation
- Business rule enforcement

Run unit tests:

```bash
cargo test -p hyperswitch_connectors -- wave::transformers::tests
```

### Integration Tests

Integration tests validate end-to-end functionality:

- Payment processing with aggregated merchants
- Backward compatibility
- Concurrent payment handling
- Configuration validation
- Performance benchmarks

Run integration tests:

```bash
cargo test -p router -- connectors::wave
```

### Test Configuration

Update your test authentication configuration in `wave_test_auth.toml`:

```toml
[wave.test_aggregated_merchants]
api_key = "sk_test_your_test_key_here"
key1 = '''
{
  "enabled": true,
  "auto_create_on_profile_creation": true,
  "default_business_type": "ecommerce",
  "cache_ttl_seconds": 1800
}
'''
```

## Migration Guide

### Backward Compatibility

The aggregated merchants feature is fully backward compatible:

- Existing Wave connector configurations continue to work unchanged
- No breaking changes to existing API contracts
- Payments without aggregated merchant configuration process normally
- Graceful handling of missing aggregated merchant metadata

### Migration Steps

1. **Update Configuration** (Optional)
   - Add aggregated merchant configuration to connector authentication
   - Enable feature flag in configuration

2. **Configure Business Profiles** (Optional)
   - Add Wave aggregated merchant metadata to relevant business profiles
   - Configure auto-creation settings as needed

3. **Validate Configuration**
   - Test payment processing with existing profiles
   - Verify aggregated merchant resolution works correctly
   - Confirm error handling and fallback behavior

4. **Monitor Performance**
   - Monitor payment processing latency
   - Check aggregated merchant API usage
   - Verify cache performance metrics

### Rollback Procedure

If issues arise, you can safely rollback:

1. **Disable Feature**: Set `enabled: false` in connector configuration
2. **Remove Metadata**: Remove aggregated merchant metadata from business profiles
3. **Revert Configuration**: Revert to standard HeaderKey authentication
4. **Monitor**: Verify that payments continue processing normally

## Troubleshooting

### Common Issues

#### Aggregated Merchant Not Found

**Problem**: Error "Aggregated merchant not found"

**Solutions**:
- Verify the merchant ID is correct and starts with "am-"
- Check that the merchant exists in your Wave account
- Ensure your API key has access to the merchant
- Consider enabling auto-creation for the profile

#### Auto-Creation Fails

**Problem**: Auto-creation of aggregated merchants fails

**Solutions**:
- Verify business profile has required information
- Check that business type is specified
- Ensure API key has creation permissions
- Review business description length (max 500 chars)

#### Configuration Validation Errors

**Problem**: Invalid configuration errors

**Solutions**:
- Validate JSON format in enhanced configuration
- Check field length limits (names, descriptions, URLs)
- Verify business type is supported
- Ensure cache TTL is within valid range (60-86400 seconds)

#### Performance Issues

**Problem**: Slow payment processing with aggregated merchants

**Solutions**:
- Enable caching if disabled
- Adjust cache TTL settings
- Monitor aggregated merchant API response times
- Consider pre-resolving merchants for high-volume profiles

### Debug Configuration

Enable debug logging for aggregated merchant operations:

```rust
// In your logging configuration
router_env::logger::info!("Aggregated merchant resolution: {}", merchant_id);
```

### Monitoring Metrics

Key metrics to monitor:

- `wave_aggregated_merchant_cache_hit_rate`: Cache efficiency
- `wave_aggregated_merchant_api_errors`: API failure rate
- `wave_aggregated_merchant_creation_time`: Creation performance
- `wave_payments_with_aggregated_merchant`: Feature adoption rate

### Support

For additional support:

1. **Documentation**: Review Wave API documentation at https://docs.wave.com/
2. **Logs**: Check Hyperswitch logs for detailed error messages
3. **Monitoring**: Review metrics and performance data
4. **Testing**: Use the comprehensive test suite to validate configuration

## Best Practices

### Configuration Management

- Use environment-specific configurations for different deployment stages
- Store sensitive API keys securely using secret management systems
- Validate configuration changes in staging before production deployment
- Monitor configuration effectiveness through metrics and logs

### Performance Optimization

- Enable caching for frequently accessed aggregated merchants
- Use appropriate cache TTL values based on your update frequency
- Monitor API rate limits and implement proper retry logic
- Pre-resolve aggregated merchants for high-volume business profiles

### Security Considerations

- Rotate API keys regularly and monitor usage patterns
- Implement proper access controls for aggregated merchant configuration
- Validate all input data and sanitize business profile metadata
- Monitor for unusual aggregated merchant creation or access patterns

### Operational Excellence

- Implement comprehensive monitoring and alerting
- Use feature flags to control aggregated merchant functionality
- Maintain backup and rollback procedures
- Document configuration changes and their business impact