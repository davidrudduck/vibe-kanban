//! Variable expansion service for task descriptions.
//!
//! Expands `$VAR` and `${VAR}` syntax in text using a provided variable map.
//! Variable names must match `[A-Z][A-Z0-9_]*` (uppercase, start with letter).

use std::collections::HashMap;
use uuid::Uuid;

/// Result of variable expansion
#[derive(Debug, Clone, PartialEq)]
pub struct ExpansionResult {
    /// The text with variables expanded
    pub text: String,
    /// Variables that were referenced but not defined
    pub undefined_vars: Vec<String>,
    /// Variables that were successfully expanded (name -> source task id)
    pub expanded_vars: Vec<(String, Option<Uuid>)>,
}

/// Checks if a character is valid for the start of a variable name
fn is_var_start(c: char) -> bool {
    c.is_ascii_uppercase()
}

/// Checks if a character is valid within a variable name
fn is_var_char(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'
}

/// Expands variables in the given text using the provided variable map.
///
/// Supports two syntaxes:
/// - `$VAR` - simple variable reference
/// - `${VAR}` - braced variable reference (allows adjacent characters)
///
/// Variable names must match `[A-Z][A-Z0-9_]*`:
/// - Start with an uppercase letter
/// - Followed by uppercase letters, digits, or underscores
///
/// # Arguments
/// * `text` - The text containing variable references
/// * `variables` - Map of variable names to (value, source_task_id) tuples
///
/// # Returns
/// An `ExpansionResult` containing the expanded text and metadata about the expansion
pub fn expand_variables(
    text: &str,
    variables: &HashMap<String, (String, Option<Uuid>)>,
) -> ExpansionResult {
    let mut result = String::with_capacity(text.len());
    let mut undefined_vars = Vec::new();
    let mut expanded_vars = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            // Check for ${VAR} syntax
            if chars[i + 1] == '{' {
                if let Some((var_name, end_idx)) = parse_braced_var(&chars, i + 2) {
                    if let Some((value, source_id)) = variables.get(&var_name) {
                        result.push_str(value);
                        expanded_vars.push((var_name.clone(), *source_id));
                    } else {
                        // Leave undefined variable unexpanded
                        result.push_str(&format!("${{{}}}", var_name));
                        if !undefined_vars.contains(&var_name) {
                            undefined_vars.push(var_name);
                        }
                    }
                    i = end_idx + 1; // Skip past the closing brace
                    continue;
                }
            }
            // Check for $VAR syntax
            else if is_var_start(chars[i + 1]) {
                let (var_name, end_idx) = parse_simple_var(&chars, i + 1);
                if let Some((value, source_id)) = variables.get(&var_name) {
                    result.push_str(value);
                    expanded_vars.push((var_name.clone(), *source_id));
                } else {
                    // Leave undefined variable unexpanded
                    result.push_str(&format!("${}", var_name));
                    if !undefined_vars.contains(&var_name) {
                        undefined_vars.push(var_name);
                    }
                }
                i = end_idx;
                continue;
            }
        }
        // Not a variable reference, copy character as-is
        result.push(chars[i]);
        i += 1;
    }

    ExpansionResult {
        text: result,
        undefined_vars,
        expanded_vars,
    }
}

/// Parse a braced variable reference starting at the given index (after `${`)
/// Returns the variable name and the index of the closing brace
fn parse_braced_var(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut i = start;

    // First character must be uppercase letter
    if i >= chars.len() || !is_var_start(chars[i]) {
        return None;
    }
    name.push(chars[i]);
    i += 1;

    // Collect remaining variable name characters
    while i < chars.len() && is_var_char(chars[i]) {
        name.push(chars[i]);
        i += 1;
    }

    // Must end with closing brace
    if i < chars.len() && chars[i] == '}' {
        Some((name, i))
    } else {
        None
    }
}

/// Parse a simple variable reference starting at the given index (after `$`)
/// Returns the variable name and the index after the last character
fn parse_simple_var(chars: &[char], start: usize) -> (String, usize) {
    let mut name = String::new();
    let mut i = start;

    // First character must be uppercase letter (already validated by caller)
    if i < chars.len() && is_var_start(chars[i]) {
        name.push(chars[i]);
        i += 1;
    }

    // Collect remaining variable name characters
    while i < chars.len() && is_var_char(chars[i]) {
        name.push(chars[i]);
        i += 1;
    }

    (name, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vars(vars: &[(&str, &str)]) -> HashMap<String, (String, Option<Uuid>)> {
        vars.iter()
            .map(|(k, v)| (k.to_string(), (v.to_string(), None)))
            .collect()
    }

    fn make_vars_with_source(
        vars: &[(&str, &str, Option<Uuid>)],
    ) -> HashMap<String, (String, Option<Uuid>)> {
        vars.iter()
            .map(|(k, v, id)| (k.to_string(), (v.to_string(), *id)))
            .collect()
    }

    // Test: $VAR syntax expands correctly
    #[test]
    fn test_simple_var_expansion() {
        let vars = make_vars(&[("PLAN", "~/.claude/plans/my-plan.md")]);
        let result = expand_variables("Follow the plan $PLAN", &vars);

        assert_eq!(result.text, "Follow the plan ~/.claude/plans/my-plan.md");
        assert!(result.undefined_vars.is_empty());
        assert_eq!(result.expanded_vars.len(), 1);
        assert_eq!(result.expanded_vars[0].0, "PLAN");
    }

    // Test: ${VAR} syntax expands correctly
    #[test]
    fn test_braced_var_expansion() {
        let vars = make_vars(&[("TASK_PLAN", "/path/to/plan.md")]);
        let result = expand_variables("Use ${TASK_PLAN} for guidance", &vars);

        assert_eq!(result.text, "Use /path/to/plan.md for guidance");
        assert!(result.undefined_vars.is_empty());
    }

    // Test: Braced syntax allows adjacent characters
    #[test]
    fn test_braced_var_adjacent_chars() {
        let vars = make_vars(&[("VERSION", "1.0.0")]);
        let result = expand_variables("app-v${VERSION}-release", &vars);

        assert_eq!(result.text, "app-v1.0.0-release");
    }

    // Test: Mixed $VAR and ${VAR} in same string
    #[test]
    fn test_mixed_syntax() {
        let vars = make_vars(&[("FOO", "foo_value"), ("BAR", "bar_value")]);
        let result = expand_variables("Start $FOO middle ${BAR} end", &vars);

        assert_eq!(result.text, "Start foo_value middle bar_value end");
        assert!(result.undefined_vars.is_empty());
        assert_eq!(result.expanded_vars.len(), 2);
    }

    // Test: Undefined variable left unexpanded, added to undefined_vars
    #[test]
    fn test_undefined_variable() {
        let vars = make_vars(&[("DEFINED", "value")]);
        let result = expand_variables("Has $DEFINED and $UNDEFINED vars", &vars);

        assert_eq!(result.text, "Has value and $UNDEFINED vars");
        assert_eq!(result.undefined_vars, vec!["UNDEFINED"]);
    }

    // Test: Undefined braced variable left unexpanded
    #[test]
    fn test_undefined_braced_variable() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("Missing ${MISSING_VAR} here", &vars);

        assert_eq!(result.text, "Missing ${MISSING_VAR} here");
        assert_eq!(result.undefined_vars, vec!["MISSING_VAR"]);
    }

    // Test: Duplicate undefined variable only listed once
    #[test]
    fn test_duplicate_undefined_variable() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("$MISSING and $MISSING again", &vars);

        assert_eq!(result.text, "$MISSING and $MISSING again");
        assert_eq!(result.undefined_vars, vec!["MISSING"]);
    }

    // Test: Empty variable value expands to empty string
    #[test]
    fn test_empty_value() {
        let vars = make_vars(&[("EMPTY", "")]);
        let result = expand_variables("Before $EMPTY after", &vars);

        assert_eq!(result.text, "Before  after");
        assert!(result.undefined_vars.is_empty());
    }

    // Test: Special characters in value preserved
    #[test]
    fn test_special_chars_in_value() {
        let vars = make_vars(&[("PATH", "/home/user/file with spaces & symbols!.txt")]);
        let result = expand_variables("Open $PATH", &vars);

        assert_eq!(
            result.text,
            "Open /home/user/file with spaces & symbols!.txt"
        );
    }

    // Test: Variable names with underscores and numbers
    #[test]
    fn test_var_with_underscores_and_numbers() {
        let vars = make_vars(&[("MY_VAR_123", "value123"), ("A1_B2_C3", "abc")]);
        let result = expand_variables("$MY_VAR_123 and ${A1_B2_C3}", &vars);

        assert_eq!(result.text, "value123 and abc");
    }

    // Test: $lowercase is not a variable (must start with uppercase)
    #[test]
    fn test_lowercase_not_variable() {
        let vars = make_vars(&[("foo", "should not expand")]);
        let result = expand_variables("$foo stays as is", &vars);

        assert_eq!(result.text, "$foo stays as is");
        assert!(result.undefined_vars.is_empty()); // Not even considered undefined
    }

    // Test: $123 is not a variable (must start with letter)
    #[test]
    fn test_number_start_not_variable() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("$123 stays as is", &vars);

        assert_eq!(result.text, "$123 stays as is");
        assert!(result.undefined_vars.is_empty());
    }

    // Test: Single $ at end of string
    #[test]
    fn test_dollar_at_end() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("Cost is 100$", &vars);

        assert_eq!(result.text, "Cost is 100$");
    }

    // Test: Standalone $ followed by space
    #[test]
    fn test_dollar_followed_by_space() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("$ 100 dollars", &vars);

        assert_eq!(result.text, "$ 100 dollars");
    }

    // Test: Invalid braced syntax (no closing brace)
    #[test]
    fn test_unclosed_brace() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("${UNCLOSED text", &vars);

        // Invalid syntax is preserved as-is
        assert_eq!(result.text, "${UNCLOSED text");
    }

    // Test: Empty braces
    #[test]
    fn test_empty_braces() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("${} empty", &vars);

        assert_eq!(result.text, "${} empty");
    }

    // Test: Braced with lowercase start
    #[test]
    fn test_braced_lowercase_start() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("${lowercase} text", &vars);

        // Invalid variable name, preserved as-is
        assert_eq!(result.text, "${lowercase} text");
    }

    // Test: Multiple variables in a row
    #[test]
    fn test_adjacent_variables() {
        let vars = make_vars(&[("A", "1"), ("B", "2"), ("C", "3")]);
        let result = expand_variables("$A$B$C", &vars);

        assert_eq!(result.text, "123");
    }

    // Test: Variable at start and end of string
    #[test]
    fn test_var_at_boundaries() {
        let vars = make_vars(&[("START", "begin"), ("END", "finish")]);
        let result = expand_variables("$START middle $END", &vars);

        assert_eq!(result.text, "begin middle finish");
    }

    // Test: Text with no variables
    #[test]
    fn test_no_variables() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("Plain text without variables", &vars);

        assert_eq!(result.text, "Plain text without variables");
        assert!(result.undefined_vars.is_empty());
        assert!(result.expanded_vars.is_empty());
    }

    // Test: Empty input
    #[test]
    fn test_empty_input() {
        let vars: HashMap<String, (String, Option<Uuid>)> = HashMap::new();
        let result = expand_variables("", &vars);

        assert_eq!(result.text, "");
        assert!(result.undefined_vars.is_empty());
    }

    // Test: Source task ID is tracked in expanded_vars
    #[test]
    fn test_source_tracking() {
        let source_id = Uuid::new_v4();
        let vars = make_vars_with_source(&[("INHERITED", "value", Some(source_id))]);
        let result = expand_variables("$INHERITED", &vars);

        assert_eq!(result.text, "value");
        assert_eq!(result.expanded_vars.len(), 1);
        assert_eq!(
            result.expanded_vars[0],
            ("INHERITED".to_string(), Some(source_id))
        );
    }

    // Test: Value containing dollar sign
    #[test]
    fn test_value_with_dollar_sign() {
        let vars = make_vars(&[("PRICE", "$100")]);
        let result = expand_variables("The price is $PRICE", &vars);

        // Note: nested expansion is NOT supported in MVP
        assert_eq!(result.text, "The price is $100");
    }

    // Test: Value containing variable-like pattern
    #[test]
    fn test_value_with_var_pattern() {
        let vars = make_vars(&[("TEMPLATE", "Use $OTHER for more")]);
        let result = expand_variables("Template: $TEMPLATE", &vars);

        // No recursive expansion - value is literal
        assert_eq!(result.text, "Template: Use $OTHER for more");
    }

    // Test: Unicode in surrounding text
    #[test]
    fn test_unicode_text() {
        let vars = make_vars(&[("GREETING", "Hello")]);
        let result = expand_variables("日本語 $GREETING 中文", &vars);

        assert_eq!(result.text, "日本語 Hello 中文");
    }

    // Test: Newlines in text
    #[test]
    fn test_multiline_text() {
        let vars = make_vars(&[("VAR", "expanded")]);
        let result = expand_variables("Line 1\n$VAR\nLine 3", &vars);

        assert_eq!(result.text, "Line 1\nexpanded\nLine 3");
    }

    // Test: Tabs and whitespace
    #[test]
    fn test_whitespace() {
        let vars = make_vars(&[("VAR", "value")]);
        let result = expand_variables("\t$VAR\t", &vars);

        assert_eq!(result.text, "\tvalue\t");
    }

    // Test: Very long variable name
    #[test]
    fn test_long_variable_name() {
        let long_name = "A".repeat(100);
        let mut vars = HashMap::new();
        vars.insert(long_name.clone(), ("long_value".to_string(), None));

        let result = expand_variables(&format!("${}", long_name), &vars);

        assert_eq!(result.text, "long_value");
    }

    // Test: Practical use case - Claude plan file
    #[test]
    fn test_practical_plan_file() {
        let vars = make_vars(&[
            ("TASK_PLAN", "~/.claude/plans/feature-auth.md"),
            ("SECTION", "2"),
        ]);
        let result = expand_variables(
            "Follow the plan $TASK_PLAN and complete section $SECTION. \
             Reference ${TASK_PLAN} for context.",
            &vars,
        );

        assert_eq!(
            result.text,
            "Follow the plan ~/.claude/plans/feature-auth.md and complete section 2. \
             Reference ~/.claude/plans/feature-auth.md for context."
        );
        assert!(result.undefined_vars.is_empty());
        // TASK_PLAN expanded twice, SECTION once
        assert_eq!(result.expanded_vars.len(), 3);
    }
}
