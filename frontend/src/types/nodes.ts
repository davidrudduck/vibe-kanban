/**
 * Node types for the swarm/hive architecture.
 * These types correspond to the Rust types in crates/remote/src/nodes/domain.rs
 */

export type NodeStatus = 'pending' | 'online' | 'offline' | 'busy' | 'draining';

export interface NodeCapabilities {
  executors: string[];
  max_concurrent_tasks: number;
  os: string;
  arch: string;
  version: string;
}

export interface Node {
  id: string;
  organization_id: string;
  name: string;
  machine_id: string;
  status: NodeStatus;
  capabilities: NodeCapabilities;
  public_url: string | null;
  last_heartbeat_at: string | null;
  connected_at: string | null;
  disconnected_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface NodeProject {
  id: string;
  node_id: string;
  local_project_id: string;
  name: string;
  git_repo_path: string;
  default_branch: string;
  swarm_project_id: string | null;
  swarm_project_name: string | null;
  last_seen_at: string;
  created_at: string;
}

export interface NodeApiKey {
  id: string;
  organization_id: string;
  name: string;
  key_prefix: string;
  created_by: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
  created_at: string;
  /** The node this API key is bound to (set on first connection) */
  node_id: string | null;
  /** Number of takeover attempts within the current window */
  takeover_count: number;
  /** Start of the current takeover detection window */
  takeover_window_start: string | null;
  /** When the key was blocked due to suspected duplicate use */
  blocked_at: string | null;
  /** Reason for blocking (e.g., "Duplicate key use detected") */
  blocked_reason: string | null;
}

export interface CreateNodeApiKeyRequest {
  organization_id: string;
  name: string;
}

export interface CreateNodeApiKeyResponse {
  api_key: NodeApiKey;
  secret: string;
}

/** Response from merging two nodes */
export interface MergeNodesResponse {
  source_node_id: string;
  target_node_id: string;
  projects_moved: number;
  keys_rebound: number;
}
