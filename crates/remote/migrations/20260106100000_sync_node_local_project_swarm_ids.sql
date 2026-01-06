-- Sync swarm_project_id from swarm_project_nodes to node_local_projects
-- This fixes a bug where link operations weren't updating node_local_projects.swarm_project_id,
-- causing the Node Projects UI to show all projects as "Unlinked" even when they were linked.

-- Update node_local_projects.swarm_project_id for existing links
UPDATE node_local_projects nlp
SET swarm_project_id = spn.swarm_project_id
FROM swarm_project_nodes spn
WHERE nlp.node_id = spn.node_id
  AND nlp.local_project_id = spn.local_project_id
  AND nlp.swarm_project_id IS NULL;
