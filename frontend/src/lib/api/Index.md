# API Module

This directory contains the frontend API client utilities and endpoint definitions.

## Structure

- `utils.ts` - Shared utilities for making HTTP requests and handling responses
- `index.ts` - Re-exports from all API modules (to be populated as endpoints are migrated)

## Utilities (`utils.ts`)

### `ApiError<E>`
Custom error class for API errors with optional typed error data.

```typescript
class ApiError<E = unknown> extends Error {
  status?: number;
  error_data?: E;
  statusCode?: number;
  response?: Response;
}
```

### `REQUEST_TIMEOUT_MS`
Default request timeout constant (30 seconds).

### `makeRequest(url, options?)`
Makes an HTTP request with:
- Automatic timeout handling
- Default `Content-Type: application/json` header
- Support for combining abort signals

### `handleApiResponse<T, E>(response)`
Parses an API response and returns the data, throwing `ApiError` on failure.
Use for standard API calls where errors should be thrown.

### `handleApiResponseAsResult<T, E>(response)`
Parses an API response and returns a `Result<T, E>` type.
Use when you need to inspect typed error data instead of catching exceptions.

### Result Types
- `Ok<T>` - Success result: `{ success: true, data: T }`
- `Err<E>` - Error result: `{ success: false, error: E | undefined, message?: string }`
- `Result<T, E>` - Union of `Ok<T> | Err<E>`

## Migration Status

This module is being incrementally populated as endpoints are migrated from `lib/api.ts`.

### Phase 1: Utils (Current)
- [x] ApiError class
- [x] REQUEST_TIMEOUT_MS constant
- [x] makeRequest function
- [x] anySignal helper
- [x] Result types (Ok, Err, Result)
- [x] handleApiResponseAsResult function
- [x] handleApiResponse function

### Phase 2: Endpoint Groups (Future)
Endpoints will be migrated from `lib/api.ts` in subsequent tasks.
