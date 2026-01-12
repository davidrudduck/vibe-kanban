# Swarm Sync Architecture

## Overview

The swarm sync system enables distributed task management across multiple Vibe Kanban nodes. Each node maintains its own local data (SQLite database) while synchronizing tasks and metadata through a central Hive server.

## Sync Flow

### 1. Node Registration

When a node first connects to the Hive:

1. **Node Registration**: The node connects via WebSocket to the Hive server using its API key (`VK_NODE_API_KEY`)
2. **Authentication**: Node presents its credentials to prove ownership of its projects
3. **Project Discovery**: Hive shares swarm projects (from other nodes) with this node
4. **Local Project Linking**: Node can link its local projects to swarm projects

```text
Node A (Local)           Hive (Central)            Node B (Local)
┌─────────────┐          ┌───────────────┐          ┌─────────────┐
│ SQLite DB   │◄────►│ PostgreSQL DB  │◄────►│ SQLite DB   │
│ - Projects  │          │ - Orgs      │          │ - Projects  │
│ - Tasks     │          │ - Users      │          │ - Tasks     │
│ - Logs      │          │ - Swarm Pjs  │          │ - Logs      │
└─────────────┘          └───────────────┘          └─────────────┘
```

### 2. Task Synchronization

Tasks flow between nodes in the following manner:

1. **Task Creation on Node A**:
   - Task is stored locally in SQLite
   - If project is linked to swarm, task gets a `shared_task_id` UUID
   - Task is published to Hive via WebSocket

2. **Task Distribution**:
   - Hive receives task from Node A
   - Hive looks up which other nodes have this project linked
   - Hive assigns task to appropriate node(s) based on:
     - Node availability (online status)
     - Load balancing
     - Explicit node assignment

3. **Task Execution on Node B**:
   - Node B receives task via WebSocket
   - Task is stored locally with original `shared_task_id`
   - Node B tracks progress and updates Hive in real-time

4. **Sync Back to Hive**:
   - When Node B completes task (or makes progress)
   - Updates are sent to Hive
   - Hive propagates updates to all nodes with this project linked
   - Node A receives updates showing task completed

### 3. Conflict Resolution

When the same task is modified on multiple nodes simultaneously:

- **Last Write Wins**: The node that writes changes to Hive last wins
- **Version Detection**: Each task update includes a timestamp
- **Notification**: Other nodes receive conflict notifications and can merge/reject changes

## Orphaned Task Detection

An **orphaned task** is a task that has a `shared_task_id` but the project is no longer linked to the Hive (or the project was deleted from Hive).

### How Orphaned Tasks Occur

1. **Project Unlinked**: User manually unlinks a project from swarm
   ```rust
   // Project unlinked - remote_project_id cleared
   project.remote_project_id = None;
   ```
   Tasks that still have `shared_task_id` set are now orphaned

2. **Project Deleted from Hive**: The swarm project is removed from Hive
   - Local projects that were linked to this project lose their connection
   - Tasks with `shared_task_id` matching the deleted project become orphaned

3. **Connection Lost**: Hive connection is lost for extended period
   - Tasks can't sync but `shared_task_id` remains set
   - When connection is restored, tasks may be out of sync

### Orphaned Task Detection

The backend detects orphaned tasks by checking:

```rust
// crates/db/src/models/task/sync.rs
pub async fn count_orphaned_for_project(
    pool: &SqlitePool,
    project_id: Uuid
) -> Result<i64, sqlx::Error> {
    // Count tasks with shared_task_id WHERE project has no remote_project_id
    sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) as "count: i64"
        FROM tasks
        WHERE project_id = $1
          AND shared_task_id IS NOT NULL
          AND remote_project_id IS NULL
        "#,
        project_id
    )
    .fetch_one(pool)
    .await
}
```

### Sync Health API

The `/api/projects/{id}/sync-health` endpoint provides project sync health information:

**Request**: `GET /api/projects/{project_id}/sync-health`

**Response** (`SyncHealthResponse`):
```typescript
{
  "success": true,
  "data": {
    "is_linked": boolean,          // Is project linked to Hive?
    "remote_project_id": string | null,  // Hive project ID
    "orphaned_task_count": number,    // Number of orphaned tasks
    "has_sync_issues": boolean,     // Are there any issues?
    "issues": SyncIssue[]           // Array of specific issues
  }
}
```

**Issue Types**:
- `OrphanedTasks { count: i64 }` - Tasks with shared_task_id but no Hive link
- `ProjectNotLinked` - Project has no remote_project_id but has swarm tasks

## Unlink Process

Unlinking a project from swarm clears all sync state and makes tasks local-only.

### Unlink Flow

1. **User Requests Unlink**: Clicks "Unlink" button in Swarm Settings or Node Projects
2. **Confirmation Dialog**: User confirms the action:
   ```bash
   "Are you sure you want to unlink {project_name} from Swarm?"
   "This will clear all sync state for this project."
   "Tasks will no longer be shared with other nodes."
   ```

3. **Backend Unlink Mutation** (`POST /api/projects/{id}/unlink-swarm`):
   ```rust
   // crates/server/src/routes/projects/handlers/core.rs
   pub async fn unlink_from_swarm(
       Extension(project): Extension<Project>,
       State(deployment): State<DeploymentImpl>,
       Json(payload): Json<UnlinkSwarmRequest>,
   ) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
       // 1. Validate project can be unlinked
       // 2. Clear remote_project_id from project
       // 3. Clear shared_task_id from all project tasks
       // 4. Attempt cleanup of orphaned tasks on Hive (optional)
   }
   ```
4. **Task Cleanup**: Backend clears `shared_task_id` and related sync fields from all tasks
5. **Hive Notification**: (Optional) Notify Hive that this node is no longer participating

### Unlink Request/Response

**Request** (`UnlinkSwarmRequest`):
```typescript
{
  "notify_hive": boolean  // Whether to notify Hive server (default: false)
}
```

**Response**: Standard `ApiResponse<()>`

### Related Code

- **Handler**: `crates/server/src/routes/projects/handlers/core.rs` - `unlink_from_swarm()`
- **Task Cleanup**: `crates/db/src/models/task/cleanup.rs` - `clear_swarm_state_for_project()`
- **Frontend Component**: `frontend/src/components/swarm/NodeProjectsSection.tsx` - Unlink dialog
- **Frontend Hook**: `frontend/src/hooks/useProjectMutations.ts` - `unlinkFromSwarm` mutation

## Frontend Components

### SwarmHealthSection

Located at: `frontend/src/components/swarm/SwarmHealthSection.tsx`

Displays overall swarm health and provides bulk fix options:

```typescript
// Aggregates health across all projects
const swarmHealth = useSwarmHealth();
// Returns: {
//   totalProjects: number,
//   projectsWithIssues: number,
//   totalOrphanedTasks: number,
//   isHealthy: boolean,
//   isLoading: boolean
// }
```

**Features**:
- Hidden when no issues detected (`isHealthy === true`)
- Shows count of projects with sync issues
- Shows total orphaned tasks count
- "Fix All Issues" button to bulk unlink all broken projects
- Shows loading state during bulk operation
- Displays success/error feedback

**Bulk Fix Flow**:
1. Fetch all projects
2. Fetch sync health for each project
3. Identify projects with `has_sync_issues === true`
4. Call `unlinkFromSwarm` mutation for each broken project
5. Display results (success count, error count)

### useSwarmHealth Hook

Located at: `frontend/src/hooks/useSwarmHealth.ts`

Aggregates sync health data across all projects using React Query:

```typescript
// Fetches all projects
const projectsQuery = useQuery({
  queryKey: ['projects'],
  queryFn: () => projectsApi.getAll(),
});

// Fetches sync health for each project in parallel
const syncHealthQueries = useQueries({
  queries: projectsQuery.data?.map((project) => ({
    queryKey: ['project', project.id, 'sync-health'],
    queryFn: () => projectsApi.getSyncHealth(project.id),
    enabled: projectsQuery.isSuccess && !!project.id,
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
  })) || [],
});

// Aggregates results
let projectsWithIssues = 0;
let totalOrphanedTasks = 0;

for (const query of syncHealthQueries) {
  if (query.data?.has_sync_issues) {
    projectsWithIssues++;
  }
  if (query.data?.orphaned_task_count) {
    totalOrphanedTasks += Number(query.data.orphaned_task_count);
  }
}
```

### useProjectSyncHealth Hook

Located at: `frontend/src/hooks/useProjectSyncHealth.ts`

Fetches sync health for a single project:

```typescript
export function useProjectSyncHealth(projectId: string) {
  return useQuery({
    queryKey: ['project', projectId, 'sync-health'],
    queryFn: () => projectsApi.getSyncHealth(projectId),
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
  });
}
```

### SyncHealthIndicator Component

Located at: `frontend/src/components/projects/SyncHealthIndicator.tsx`

Displays sync health status on project cards:

```typescript
// Shows sync status
export function SyncHealthIndicator({ projectId }: Props) {
  const { data } = useProjectSyncHealth(projectId);

  // Orphaned tasks warning
  if (data?.has_sync_issues) {
    return <AlertTriangle className="text-amber-500" />;
  }

  // Healthy state (optional indicator)
  return null;
}
```

## Database Schema

### Sync-Related Tables

**projects** table:
```sql
-- Key sync fields
remote_project_id        UUID    -- ID of project in Hive (NULL = not linked)
remote_last_synced_at   DATETIME -- Last successful sync timestamp
source_node_id         UUID    -- Node that created/owns this project
source_node_name        TEXT     -- Human-readable node name
source_node_status     TEXT     -- 'online' | 'offline' | 'unknown'
source_node_public_url  TEXT     -- URL for direct log streaming
```

**tasks** table:
```sql
-- Sync fields
shared_task_id         UUID    -- Global task identifier across swarm
remote_last_synced_at   DATETIME -- Last time task was synced from Hive
remote_assigned_node_id UUID    -- Node currently executing this task (assigned by Hive)
```

## Error Cases and Recovery

### Project Not Found on Unlink

**Symptom**: When unlinking, error "Project not found"

**Cause**: Project was deleted or doesn't belong to user's organization

**Recovery**: Refresh project list before attempting unlink

### Hive Connection Issues

**Symptoms**:
- Tasks not syncing between nodes
- "task sync failed" errors in logs
- Remote last sync timestamp not updating

**Recovery**:
1. Check `VK_HIVE_URL` environment variable
2. Verify `VK_NODE_API_KEY` is valid
3. Check Hive server status (if accessible)
4. Restart node application to re-establish WebSocket connection

### Orphaned Task Cleanup

**Symptom**: Tasks show as orphaned even after project is re-linked

**Cause**: `shared_task_id` wasn't cleared on re-link

**Recovery**: Manually clear sync state before re-linking:
1. Unlink project completely
2. Re-link to swarm project
3. This clears all stale sync state

## Security Considerations

### API Key Management

- API keys are stored in environment: `VK_NODE_API_KEY`
- Never log or trace API keys
- Keys can be rotated via Hive web interface
- Different nodes can use different API keys for the same organization

### WebSocket Security

- WebSocket connections use TLS/WSS for encrypted transport
- Message payload is limited to 1MB
- Rate limiting applies to prevent abuse
- Each connection is authenticated with API key on handshake

## Testing

### Sync Health Endpoint

Test the sync health endpoint:

```bash
# Get sync health for a project
curl http://localhost:3001/api/projects/{project_id}/sync-health

# Expected response for healthy project
{
  "success": true,
  "data": {
    "is_linked": true,
    "remote_project_id": "uuid-here",
    "orphaned_task_count": 0,
    "has_sync_issues": false,
    "issues": []
  }
}

# Expected response for project with orphaned tasks
{
  "success": true,
  "data": {
    "is_linked": true,
    "remote_project_id": "uuid-here",
    "orphaned_task_count": 5,
    "has_sync_issues": true,
    "issues": [
      { "OrphanedTasks": { "count": 5 } }
    ]
  }
}
```

### Unlink Operation

Test unlinking a project:

```bash
curl -X POST http://localhost:3001/api/projects/{project_id}/unlink-swarm \
  -H "Content-Type: application/json" \
  -d '{ "notify_hive": false }'

# Expected response
{
  "success": true,
  "data": {},
  "message": "Project unlinked from swarm"
}
```

## Related Code Files

| Component/File | Path | Purpose |
|--------------|------|---------|
| Sync Health Handler | `crates/server/src/routes/projects/handlers/core.rs:623` | Provides project sync health info |
| Unlink Handler | `crates/server/src/routes/projects/handlers/core.rs` | Unlinks project from swarm |
| Task Cleanup | `crates/db/src/models/task/cleanup.rs` | Clears sync state from tasks |
| Orphaned Count Query | `crates/db/src/models/task/sync.rs:404` | Counts orphaned tasks |
| SyncHealthResponse Type | `crates/server/src/routes/projects/types.rs:222` | TypeScript types for sync health |
| SwarmHealthSection | `frontend/src/components/swarm/SwarmHealthSection.tsx` | Displays aggregate swarm health |
| useSwarmHealth Hook | `frontend/src/hooks/useSwarmHealth.ts` | Aggregates health data |
| useProjectSyncHealth Hook | `frontend/src/hooks/useProjectSyncHealth.ts` | Single project health |
| NodeProjectsSection | `frontend/src/components/swarm/NodeProjectsSection.tsx:650` | Project linking/unlinking UI |
| SwarmSettings | `frontend/src/pages/settings/SwarmSettings.tsx:105` | Settings page with health section |
| SyncHealthIndicator | `frontend/src/components/projects/SyncHealthIndicator.tsx` | Project card health indicator |
