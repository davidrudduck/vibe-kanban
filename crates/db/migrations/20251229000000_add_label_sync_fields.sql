-- Add sync fields to labels table for Hive synchronization
-- These fields track the relationship between local labels and their Hive counterparts

-- shared_label_id: UUID of the label in the Hive (NULL if not yet synced)
ALTER TABLE labels ADD COLUMN shared_label_id BLOB;

-- version: Optimistic locking version for conflict resolution
ALTER TABLE labels ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

-- synced_at: Timestamp of last successful sync to Hive
ALTER TABLE labels ADD COLUMN synced_at TEXT;

-- Index for finding unsynced labels
CREATE INDEX idx_labels_synced_at ON labels(synced_at);

-- Index for looking up labels by their Hive ID
CREATE UNIQUE INDEX idx_labels_shared_label_id ON labels(shared_label_id) WHERE shared_label_id IS NOT NULL;
