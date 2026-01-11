# API Module

This directory contains the frontend API client utilities organized by domain.

## Structure

### Core Utilities
- `utils.ts` - Shared utilities for making HTTP requests and handling responses
- `index.ts` - Re-exports all API modules for convenient importing

### API Modules by Domain

#### Core Resources
- `projects.ts` - Project CRUD operations (`projectsApi`)
- `tasks.ts` - Task CRUD, archive, labels, streaming (`tasksApi`)
- `attempts.ts` - Task attempt lifecycle, git ops, GitHub integration (`attemptsApi`)
- `taskVariables.ts` - Task variable management (`taskVariablesApi`)
- `commits.ts` - Commit operations (`commitsApi`)
- `labels.ts` - Label management (`labelsApi`)
- `templates.ts` - Project templates (`templatesApi`)

#### System Operations
- `health.ts` - Health check endpoints (`healthApi`)
- `config.ts` - App configuration (`configApi`)
- `backups.ts` - Database backup management (`backupsApi`)
- `diagnostics.ts` - System diagnostics (`diagnosticsApi`)
- `processes.ts` - System process management (`processesApi`)
- `logs.ts` - Log retrieval and pagination (`logsApi`)

#### File Operations
- `filesystem.ts` - File system and browser APIs (`fileSystemApi`, `fileBrowserApi`)
- `images.ts` - Image handling (`imagesApi`)

#### Execution
- `execution.ts` - Execution processes (`executionProcessesApi`)
- `profiles.ts` - Executor profiles (`profilesApi`)
- `mcp.ts` - MCP server management (`mcpServersApi`)
- `terminal.ts` - Terminal sessions (`terminalApi`)
- `messageQueue.ts` - Message queue operations (`messageQueueApi`)

#### Authentication & Users
- `oauth.ts` - OAuth flow (`oauthApi`)
- `organizations.ts` - Organization management (`organizationsApi`)
- `approvals.ts` - Approval workflows (`approvalsApi`)

#### Swarm/Hive Architecture
- `dashboard.ts` - Swarm dashboard metrics (`dashboardApi`)
- `nodes.ts` - Swarm node management (`nodesApi`)
- `swarmProjects.ts` - Swarm project sync (`swarmProjectsApi`)
- `swarmLabels.ts` - Swarm label sync (`swarmLabelsApi`)
- `swarmTemplates.ts` - Swarm template sync (`swarmTemplatesApi`)

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

## Usage

Import from the index for convenient access:

```typescript
import { projectsApi, tasksApi, ApiError, makeRequest } from '@/lib/api';

// Use namespace APIs
const projects = await projectsApi.list();
const task = await tasksApi.get(taskId);

// Handle errors
try {
  await tasksApi.delete(taskId);
} catch (e) {
  if (e instanceof ApiError) {
    console.error('API error:', e.message, e.status);
  }
}
```
