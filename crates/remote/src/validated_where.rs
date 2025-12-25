//! Server-side WHERE clause builder for Electric Shape API.
//!
//! This module provides a simple struct for building WHERE clauses
//! that are controlled server-side, ensuring security against
//! client-side WHERE clause injection attacks.

/// A server-side controlled WHERE clause for Electric Shape requests.
///
/// This struct encapsulates table and WHERE clause information that
/// is set server-side (not from client input) for security.
#[derive(Debug, Clone)]
pub struct ValidatedWhere {
    pub table: &'static str,
    pub where_clause: &'static str,
}

impl ValidatedWhere {
    /// Creates a new ValidatedWhere with the given table and where clause.
    ///
    /// # Arguments
    ///
    /// * `table` - The database table name (must be a static string for security)
    /// * `where_clause` - The WHERE clause with parameterized placeholders ($1, $2, etc.)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let query = ValidatedWhere::new(
    ///     "shared_tasks",
    ///     r#""organization_id" = ANY($1)"#
    /// );
    /// ```
    pub const fn new(table: &'static str, where_clause: &'static str) -> Self {
        Self {
            table,
            where_clause,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validated_where_new() {
        let query = ValidatedWhere::new("shared_tasks", r#""organization_id" = ANY($1)"#);
        assert_eq!(query.table, "shared_tasks");
        assert_eq!(query.where_clause, r#""organization_id" = ANY($1)"#);
    }

    #[test]
    fn test_validated_where_debug() {
        let query = ValidatedWhere::new("tasks", r#""project_id" = $1"#);
        let debug_str = format!("{:?}", query);
        assert!(debug_str.contains("tasks"));
        assert!(debug_str.contains("project_id"));
    }

    #[test]
    fn test_validated_where_clone() {
        let query = ValidatedWhere::new("shared_tasks", r#""id" = $1"#);
        let cloned = query.clone();
        assert_eq!(query.table, cloned.table);
        assert_eq!(query.where_clause, cloned.where_clause);
    }
}
