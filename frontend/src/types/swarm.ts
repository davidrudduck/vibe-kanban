// Types for Swarm Projects, Labels, and Templates
// These are organization-wide entities managed through the Hive

import type { JsonValue } from 'shared/types';

// =====================
// Swarm Projects
// =====================

export interface SwarmProject {
  id: string;
  organization_id: string;
  name: string;
  description: string | null;
  metadata: JsonValue;
  created_at: string;
  updated_at: string;
}

export interface SwarmTaskCounts {
  todo: number;
  in_progress: number;
  in_review: number;
  done: number;
  cancelled: number;
}

export interface SwarmProjectWithNodes {
  id: string;
  organization_id: string;
  name: string;
  description: string | null;
  metadata: JsonValue;
  created_at: string;
  updated_at: string;
  linked_nodes_count: number;
  linked_node_names: string[];
  hive_project_ids: string[];
  task_counts: SwarmTaskCounts;
}

export interface SwarmProjectNode {
  id: string;
  swarm_project_id: string;
  node_id: string;
  local_project_id: string;
  git_repo_path: string;
  os_type: string | null;
  linked_at: string;
}

export interface CreateSwarmProjectRequest {
  organization_id: string;
  name: string;
  description?: string | null;
  metadata?: JsonValue;
}

export interface UpdateSwarmProjectRequest {
  name?: string;
  description?: string | null;
  metadata?: JsonValue;
}

export interface MergeSwarmProjectsRequest {
  source_id: string;
}

export interface LinkSwarmProjectNodeRequest {
  node_id: string;
  local_project_id: string;
  git_repo_path: string;
  os_type?: string | null;
}

// API Response types
export interface SwarmProjectResponse {
  project: SwarmProject;
}

export interface ListSwarmProjectsResponse {
  projects: SwarmProjectWithNodes[];
}

export interface SwarmProjectNodeResponse {
  link: SwarmProjectNode;
}

export interface ListSwarmProjectNodesResponse {
  nodes: SwarmProjectNode[];
}

// =====================
// Swarm Labels
// =====================

export interface SwarmLabel {
  id: string;
  organization_id: string;
  project_id: string | null; // null for org-global labels
  name: string;
  color: string;
  icon: string | null;
  metadata: JsonValue;
  created_at: string;
  updated_at: string;
}

export interface CreateSwarmLabelRequest {
  organization_id: string;
  name: string;
  color: string;
  icon?: string | null;
  metadata?: JsonValue;
}

export interface UpdateSwarmLabelRequest {
  name?: string;
  color?: string;
  icon?: string | null;
  metadata?: JsonValue;
}

export interface MergeSwarmLabelsRequest {
  source_id: string;
}

export interface PromoteLabelToOrgRequest {
  label_id: string;
}

export interface ListSwarmLabelsResponse {
  labels: SwarmLabel[];
}

export interface SwarmLabelResponse {
  label: SwarmLabel;
}

// =====================
// Swarm Templates
// =====================

export interface SwarmTemplate {
  id: string;
  organization_id: string;
  name: string;
  content: string;
  metadata: JsonValue;
  created_at: string;
  updated_at: string;
}

export interface CreateSwarmTemplateRequest {
  organization_id: string;
  name: string;
  content: string;
  metadata?: JsonValue;
}

export interface UpdateSwarmTemplateRequest {
  name?: string;
  content?: string;
  metadata?: JsonValue;
}

export interface MergeSwarmTemplatesRequest {
  source_id: string;
}

export interface ListSwarmTemplatesResponse {
  templates: SwarmTemplate[];
}

export interface SwarmTemplateResponse {
  template: SwarmTemplate;
}
