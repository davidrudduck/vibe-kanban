//! Plan parsing service for extracting structured steps from Claude's plan text.
//!
//! This service parses various text formats that Claude may use when presenting
//! implementation plans, including numbered lists, markdown headers, and bullet points.
//!
//! When parsing large plan files (like full Claude plan mode documents), the parser
//! looks for a specific "Subtasks" section to extract only the relevant steps,
//! avoiding parsing the entire document as steps.

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Markers that indicate the start of a subtasks section in plan text.
/// The parser will look for these headers and only parse content within that section.
const SUBTASK_MARKERS: &[&str] = &[
    "## Subtasks",
    "## Implementation Steps",
    "## Steps",
    "## Plan Steps",
    "## Tasks",
];

/// Maximum reasonable size for a subtasks section (in bytes).
/// If the extracted section exceeds this, it's likely the wrong content.
const MAX_SUBTASKS_SECTION_SIZE: usize = 10000;

/// A parsed step from a plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedPlanStep {
    /// Order of the step (1-indexed).
    pub sequence_order: i32,
    /// Title/heading of the step.
    pub title: String,
    /// Optional description/body text for the step.
    pub description: Option<String>,
}

/// Stateless service for parsing plan text into structured steps.
#[derive(Clone, Default)]
pub struct PlanParser;

/// Detected format of the plan text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanFormat {
    /// Numbered list: "1. ", "2. ", etc.
    NumberedList,
    /// Markdown headers: "## Step 1:", "### Phase 1:", etc.
    MarkdownHeaders,
    /// Bullet points: "- ", "* "
    BulletPoints,
    /// No recognizable format
    Unknown,
}

impl PlanParser {
    pub fn new() -> Self {
        Self
    }

    /// Extract the subtasks section from a plan document.
    ///
    /// Looks for known section headers (like "## Subtasks", "## Implementation Steps")
    /// and extracts only the content within that section, stopping at the next
    /// level-2 header or end of document.
    ///
    /// Returns `None` if no subtasks section is found.
    pub fn extract_subtasks_section(plan_text: &str) -> Option<&str> {
        for marker in SUBTASK_MARKERS {
            if let Some(start_idx) = plan_text.find(marker) {
                // Start after the marker line
                let section_start = start_idx + marker.len();
                let section_text = &plan_text[section_start..];

                // Find the next ## header (same level or higher) or end of document
                let end_idx = section_text
                    .find("\n## ")
                    .or_else(|| section_text.find("\n# "))
                    .unwrap_or(section_text.len());

                let extracted = &section_text[..end_idx];

                // Sanity check: if section is too large, something is wrong
                if extracted.len() > MAX_SUBTASKS_SECTION_SIZE {
                    warn!(
                        marker = %marker,
                        section_length = extracted.len(),
                        max_size = MAX_SUBTASKS_SECTION_SIZE,
                        "Subtasks section is very large, this may indicate incorrect parsing"
                    );
                }

                return Some(extracted);
            }
        }
        None
    }

    /// Parse plan text into structured steps.
    ///
    /// If the plan text contains a recognized subtasks section (like "## Subtasks"),
    /// only that section will be parsed. Otherwise, the entire text is parsed.
    ///
    /// Supports multiple formats:
    /// - Numbered lists: "1. ", "2. ", etc.
    /// - Markdown headers: "## Step 1:", "### Phase 1:"
    /// - Bullet points: "- ", "* "
    ///
    /// Returns an empty vector if no parseable structure is found.
    pub fn parse(plan_text: &str) -> Vec<ParsedPlanStep> {
        if plan_text.trim().is_empty() {
            return Vec::new();
        }

        // Try to extract just the subtasks section first
        let text_to_parse = Self::extract_subtasks_section(plan_text).unwrap_or(plan_text);

        // If the section is still very large, skip parsing to avoid creating too many steps
        if text_to_parse.len() > MAX_SUBTASKS_SECTION_SIZE {
            warn!(
                text_length = text_to_parse.len(),
                "Plan text too large to parse, skipping to avoid creating excessive steps"
            );
            return Vec::new();
        }

        Self::parse_content(text_to_parse)
    }

    /// Parse content into steps (internal implementation).
    fn parse_content(plan_text: &str) -> Vec<ParsedPlanStep> {
        let format = Self::detect_format(plan_text);
        match format {
            PlanFormat::NumberedList => Self::parse_numbered_list(plan_text),
            PlanFormat::MarkdownHeaders => Self::parse_markdown_headers(plan_text),
            PlanFormat::BulletPoints => Self::parse_bullet_points(plan_text),
            PlanFormat::Unknown => Vec::new(),
        }
    }

    /// Detect the format by scanning the first few lines.
    fn detect_format(plan_text: &str) -> PlanFormat {
        let numbered_re = Regex::new(r"^\d+\.\s+").unwrap();
        let header_re = Regex::new(r"^#{2,3}\s+").unwrap();
        let bullet_re = Regex::new(r"^[-*]\s+").unwrap();

        // Count matches for each format in first 10 non-empty lines
        let mut numbered_count = 0;
        let mut header_count = 0;
        let mut bullet_count = 0;

        for line in plan_text.lines().filter(|l| !l.trim().is_empty()).take(10) {
            let trimmed = line.trim();
            if numbered_re.is_match(trimmed) {
                numbered_count += 1;
            }
            if header_re.is_match(trimmed) {
                header_count += 1;
            }
            if bullet_re.is_match(trimmed) {
                bullet_count += 1;
            }
        }

        // Return format with most matches, preferring numbered > headers > bullets
        if numbered_count > 0 && numbered_count >= header_count && numbered_count >= bullet_count {
            PlanFormat::NumberedList
        } else if header_count > 0 && header_count >= bullet_count {
            PlanFormat::MarkdownHeaders
        } else if bullet_count > 0 {
            PlanFormat::BulletPoints
        } else {
            PlanFormat::Unknown
        }
    }

    /// Parse numbered list format: "1. Title\nDescription\n\n2. Title\n..."
    fn parse_numbered_list(plan_text: &str) -> Vec<ParsedPlanStep> {
        let split_re = Regex::new(r"(?m)^\d+\.\s+").unwrap();
        Self::parse_with_pattern(plan_text, &split_re)
    }

    /// Parse markdown header format: "## Step 1: Title\nDescription\n\n## Step 2:..."
    fn parse_markdown_headers(plan_text: &str) -> Vec<ParsedPlanStep> {
        let split_re = Regex::new(r"(?m)^#{2,3}\s+").unwrap();
        Self::parse_with_pattern(plan_text, &split_re)
    }

    /// Parse bullet point format: "- Title\nDescription\n\n- Title\n..."
    fn parse_bullet_points(plan_text: &str) -> Vec<ParsedPlanStep> {
        let split_re = Regex::new(r"(?m)^[-*]\s+").unwrap();
        Self::parse_with_pattern(plan_text, &split_re)
    }

    /// Generic parser that splits on a pattern and extracts title/description.
    fn parse_with_pattern(plan_text: &str, pattern: &Regex) -> Vec<ParsedPlanStep> {
        let mut steps = Vec::new();

        // Find all match positions
        let matches: Vec<_> = pattern.find_iter(plan_text).collect();
        if matches.is_empty() {
            return steps;
        }

        // Extract sections between matches
        for (i, m) in matches.iter().enumerate() {
            let start = m.end();
            let end = matches.get(i + 1).map(|next| next.start()).unwrap_or(plan_text.len());

            let section = &plan_text[start..end];
            if let Some(step) = Self::parse_section(section, (i + 1) as i32) {
                steps.push(step);
            }
        }

        steps
    }

    /// Parse a single section into a ParsedPlanStep.
    /// First line = title, remaining lines = description.
    fn parse_section(section: &str, sequence_order: i32) -> Option<ParsedPlanStep> {
        let lines: Vec<&str> = section.lines().collect();
        if lines.is_empty() {
            return None;
        }

        // First line is the title
        let title = lines[0].trim();
        if title.is_empty() {
            return None;
        }

        // Clean up the title (remove trailing colons, "Step N:" prefixes, etc.)
        let title = Self::clean_title(title);
        if title.is_empty() {
            return None;
        }

        // Remaining lines form the description
        let description_lines: Vec<&str> = lines[1..]
            .iter()
            .map(|l| l.trim())
            .collect();

        // Join and trim the description
        let description = description_lines.join("\n").trim().to_string();
        let description = if description.is_empty() {
            None
        } else {
            Some(description)
        };

        Some(ParsedPlanStep {
            sequence_order,
            title,
            description,
        })
    }

    /// Clean up title text by removing common prefixes/suffixes.
    fn clean_title(title: &str) -> String {
        let mut cleaned = title.to_string();

        // Remove "Step N:" or "Phase N:" prefixes (case insensitive)
        let step_prefix_re = Regex::new(r"(?i)^(step|phase)\s*\d+\s*:\s*").unwrap();
        cleaned = step_prefix_re.replace(&cleaned, "").to_string();

        // Trim any leading/trailing whitespace and colons
        cleaned = cleaned.trim().trim_end_matches(':').trim().to_string();

        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numbered_list() {
        let plan = "1. Create database migration\nAdd the schema\n\n2. Create API routes\nImplement CRUD";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].sequence_order, 1);
        assert_eq!(steps[0].title, "Create database migration");
        assert_eq!(steps[0].description, Some("Add the schema".to_string()));
        assert_eq!(steps[1].sequence_order, 2);
        assert_eq!(steps[1].title, "Create API routes");
        assert_eq!(steps[1].description, Some("Implement CRUD".to_string()));
    }

    #[test]
    fn test_parse_numbered_list_multiline_description() {
        let plan = "1. Create database migration\nAdd the schema\nInclude indexes\n\n2. Create API routes";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "Create database migration");
        assert_eq!(
            steps[0].description,
            Some("Add the schema\nInclude indexes".to_string())
        );
    }

    #[test]
    fn test_parse_markdown_headers() {
        let plan = "## Step 1: Database\nCreate tables\n\n## Step 2: API\nCreate routes";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].sequence_order, 1);
        assert_eq!(steps[0].title, "Database");
        assert_eq!(steps[0].description, Some("Create tables".to_string()));
        assert_eq!(steps[1].sequence_order, 2);
        assert_eq!(steps[1].title, "API");
        assert_eq!(steps[1].description, Some("Create routes".to_string()));
    }

    #[test]
    fn test_parse_markdown_headers_h3() {
        let plan = "### Phase 1: Setup\nInitialize project\n\n### Phase 2: Implementation\nWrite code";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "Setup");
        assert_eq!(steps[1].title, "Implementation");
    }

    #[test]
    fn test_parse_bullet_points() {
        let plan = "- First task\nDetails here\n\n- Second task\nMore details";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].sequence_order, 1);
        assert_eq!(steps[0].title, "First task");
        assert_eq!(steps[0].description, Some("Details here".to_string()));
        assert_eq!(steps[1].sequence_order, 2);
        assert_eq!(steps[1].title, "Second task");
        assert_eq!(steps[1].description, Some("More details".to_string()));
    }

    #[test]
    fn test_parse_bullet_points_asterisk() {
        let plan = "* First task\nDetails\n\n* Second task";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "First task");
        assert_eq!(steps[1].title, "Second task");
    }

    #[test]
    fn test_empty_plan() {
        let steps = PlanParser::parse("");
        assert!(steps.is_empty());
    }

    #[test]
    fn test_whitespace_only_plan() {
        let steps = PlanParser::parse("   \n\n   \t  ");
        assert!(steps.is_empty());
    }

    #[test]
    fn test_no_parseable_structure() {
        let plan = "This is just some text without any structure.";
        let steps = PlanParser::parse(plan);
        assert!(steps.is_empty());
    }

    #[test]
    fn test_title_without_description() {
        let plan = "1. First step\n\n2. Second step";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "First step");
        assert!(steps[0].description.is_none());
        assert_eq!(steps[1].title, "Second step");
        assert!(steps[1].description.is_none());
    }

    #[test]
    fn test_mixed_format_prefers_numbered() {
        // When both numbered and bullets are present, numbered should win
        let plan = "1. First numbered\nDesc\n\n- A bullet\nInfo\n\n2. Second numbered";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "First numbered");
        assert_eq!(steps[1].title, "Second numbered");
    }

    #[test]
    fn test_real_world_claude_plan() {
        let plan = r#"## Step 1: Create database migration
Add the new `plan_steps` table with foreign key to `task_attempts`.

## Step 2: Create Rust model
Implement the SQLx model with CRUD operations.

## Step 3: Add API endpoints
Create routes for managing plan steps.

## Step 4: Update frontend
Add UI components to display plan steps."#;

        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].title, "Create database migration");
        assert!(steps[0]
            .description
            .as_ref()
            .unwrap()
            .contains("plan_steps"));
        assert_eq!(steps[3].title, "Update frontend");
    }

    #[test]
    fn test_numbered_with_periods_in_content() {
        let plan = "1. Install v2.0 of the package\nRun npm install\n\n2. Configure settings\nEdit config.json";
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "Install v2.0 of the package");
    }

    #[test]
    fn test_detect_format_numbered() {
        let plan = "1. First\n2. Second\n3. Third";
        assert_eq!(PlanParser::detect_format(plan), PlanFormat::NumberedList);
    }

    #[test]
    fn test_detect_format_headers() {
        let plan = "## First\n## Second\n## Third";
        assert_eq!(PlanParser::detect_format(plan), PlanFormat::MarkdownHeaders);
    }

    #[test]
    fn test_detect_format_bullets() {
        let plan = "- First\n- Second\n- Third";
        assert_eq!(PlanParser::detect_format(plan), PlanFormat::BulletPoints);
    }

    #[test]
    fn test_detect_format_unknown() {
        let plan = "Just some text\nWith multiple lines\nBut no structure";
        assert_eq!(PlanParser::detect_format(plan), PlanFormat::Unknown);
    }

    // Tests for subtasks section extraction

    #[test]
    fn test_extract_subtasks_section_basic() {
        let plan = r#"# My Plan

Some introduction text here.

## Subtasks

1. First task
   Description of first task

2. Second task
   Description of second task

## Notes

Some notes here.
"#;
        let section = PlanParser::extract_subtasks_section(plan);
        assert!(section.is_some());
        let section = section.unwrap();
        assert!(section.contains("First task"));
        assert!(section.contains("Second task"));
        assert!(!section.contains("Some notes here"));
        assert!(!section.contains("introduction"));
    }

    #[test]
    fn test_extract_subtasks_section_implementation_steps() {
        let plan = r#"# Plan

## Implementation Steps

1. Step one
2. Step two

## Other Section
"#;
        let section = PlanParser::extract_subtasks_section(plan);
        assert!(section.is_some());
        assert!(section.unwrap().contains("Step one"));
    }

    #[test]
    fn test_extract_subtasks_section_not_found() {
        let plan = r#"# Plan

## Introduction

Some text here.

## Conclusion

More text.
"#;
        let section = PlanParser::extract_subtasks_section(plan);
        assert!(section.is_none());
    }

    #[test]
    fn test_parse_with_subtasks_section() {
        let plan = r#"# Large Plan Document

This is a very long introduction with lots of text.
It contains many paragraphs and sections.

## Background

- Point one about the background
- Point two about the background
- Point three about the background

## Requirements

1. Requirement one that looks like a step
2. Requirement two that looks like a step
3. Requirement three that looks like a step

## Subtasks

1. Create database migration
   Add the schema for the new feature.

2. Implement API endpoints
   Create REST endpoints for CRUD operations.

3. Add frontend components
   Build React components for the UI.

## Notes

- Note one
- Note two
"#;
        // Should only parse the 3 subtasks, not the requirements or notes
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].title, "Create database migration");
        assert_eq!(steps[1].title, "Implement API endpoints");
        assert_eq!(steps[2].title, "Add frontend components");
    }

    #[test]
    fn test_parse_falls_back_to_full_text() {
        // When no subtasks section exists, parse the whole document
        let plan = r#"1. First task
Description

2. Second task
Description
"#;
        let steps = PlanParser::parse(plan);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].title, "First task");
        assert_eq!(steps[1].title, "Second task");
    }

    #[test]
    fn test_extract_subtasks_section_at_end_of_document() {
        let plan = r#"# Plan

## Introduction

Some text.

## Subtasks

1. Only task
   Description here.
"#;
        let section = PlanParser::extract_subtasks_section(plan);
        assert!(section.is_some());
        assert!(section.unwrap().contains("Only task"));
    }

    #[test]
    fn test_extract_subtasks_section_stops_at_h1() {
        let plan = r#"## Subtasks

1. First task

# New Top-Level Section

This should not be included.
"#;
        let section = PlanParser::extract_subtasks_section(plan);
        assert!(section.is_some());
        let section = section.unwrap();
        assert!(section.contains("First task"));
        assert!(!section.contains("New Top-Level Section"));
    }
}
