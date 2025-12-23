-- Rename tags table to templates
-- This migration renames the 'tags' table back to 'templates' for consistency

-- Create new templates table
CREATE TABLE templates (
    id            BLOB PRIMARY KEY,
    template_name TEXT NOT NULL CHECK(INSTR(template_name, ' ') = 0),
    content       TEXT NOT NULL CHECK(content != ''),
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

-- Copy data from tags to templates with column rename
INSERT INTO templates (id, template_name, content, created_at, updated_at)
SELECT id, tag_name, content, created_at, updated_at
FROM tags;

-- Drop the old tags table
DROP TABLE tags;
