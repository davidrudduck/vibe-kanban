-- Enforce a single coding_agent_turn per execution_process at the DB level.
-- Callers treat `execution_process_id` as a unique key (lookup-by-id,
-- batched lookup-by-ids returning a HashMap), but nothing in schema
-- currently prevents duplicates. Add a unique index so any accidental
-- duplicate insert fails loudly instead of silently dropping one row
-- in the HashMap merge.
--
-- Safety: deduplicate any pre-existing rows before creating the index
-- (keeps the most recently created row per execution_process_id so the
-- migration does not abort on databases that already have duplicates).
DELETE FROM coding_agent_turns
WHERE rowid NOT IN (
    SELECT MAX(rowid)
    FROM coding_agent_turns
    GROUP BY execution_process_id
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_coding_agent_turns_execution_process_id_unique
    ON coding_agent_turns(execution_process_id);
