# Activity API

## GET /v1/activity

Fetch activity events for a project.

### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `swarm_project_id` | UUID | Conditional* | Swarm project UUID (preferred) |
| `project_id` | UUID | Conditional* | Legacy project UUID (deprecated) |
| `after` | integer | No | Fetch events after this sequence number (pagination) |
| `limit` | integer | No | Maximum events to return (default: 50, max: 100) |

*Either `swarm_project_id` or `project_id` is required. If both are provided, `swarm_project_id` takes precedence.

**Recommended:** Use `swarm_project_id` for all new integrations. The `project_id` parameter is deprecated and will be removed in a future version.

### Response

```json
{
  "data": [
    {
      "seq": 12345,
      "event_id": "uuid",
      "project_id": "uuid",
      "event_type": "task_created",
      "created_at": "2026-01-20T12:00:00Z",
      "payload": { }
    }
  ]
}
```

### Examples

#### Fetch activity by swarm project (recommended)
```bash
curl -H "Authorization: Bearer TOKEN" \
  "https://api.example.com/v1/activity?swarm_project_id=550e8400-e29b-41d4-a716-446655440000&limit=10"
```

#### Fetch activity by legacy project ID (deprecated)
```bash
curl -H "Authorization: Bearer TOKEN" \
  "https://api.example.com/v1/activity?project_id=6ba7b810-9dad-11d1-80b4-00c04fd430c8&limit=10"
```

#### Pagination
```bash
# Fetch first page
curl "https://api.example.com/v1/activity?swarm_project_id=550e8400-e29b-41d4-a716-446655440000&limit=10"

# Fetch next page using last seq from previous response
curl "https://api.example.com/v1/activity?swarm_project_id=550e8400-e29b-41d4-a716-446655440000&after=12345&limit=10"
```

### Error Responses

| Status Code | Description |
|-------------|-------------|
| 400 Bad Request | Neither project_id nor swarm_project_id provided |
| 401 Unauthorized | Missing or invalid authentication |
| 403 Forbidden | User does not have access to the project |
| 404 Not Found | Project or swarm project does not exist |
| 500 Internal Server Error | Server error retrieving activity |

### Migration Guide

If you are currently using `project_id`:

1. **Identify your swarm project ID**: Query `/v1/swarm_projects` to find the swarm_project_id corresponding to your project_id
2. **Update API calls**: Replace `project_id=X` with `swarm_project_id=Y` in your requests
3. **Test thoroughly**: Verify you get the same activity events
4. **Deprecation timeline**: `project_id` support will be removed in version 2.0 (estimated Q3 2026)

For questions, see the [Architecture Documentation](../architecture/db/database-overview.mdx).
