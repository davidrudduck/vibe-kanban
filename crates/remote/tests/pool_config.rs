//! Unit tests for PostgreSQL pool configuration.
//!
//! These tests verify that the pool configuration respects environment variables
//! and uses sensible defaults.
//!
//! # Safety
//! These tests use `std::env::set_var` and `std::env::remove_var` which are unsafe
//! in Rust 2024 edition due to potential data races. The tests are marked as
//! `#[ignore]` by default and should be run with `--test-threads=1` to ensure
//! they don't interfere with each other.

use remote::db::get_max_connections;
use serial_test::serial;

/// Helper to safely set an environment variable in tests.
///
/// # Safety
/// This is safe when tests are run with `--test-threads=1`.
unsafe fn set_env(key: &str, value: &str) {
    // SAFETY: The caller guarantees single-threaded execution.
    unsafe { std::env::set_var(key, value) };
}

/// Helper to safely remove an environment variable in tests.
///
/// # Safety
/// This is safe when tests are run with `--test-threads=1`.
unsafe fn remove_env(key: &str) {
    // SAFETY: The caller guarantees single-threaded execution.
    unsafe { std::env::remove_var(key) };
}

/// Test that VK_PG_MAX_CONNECTIONS environment variable is respected.
#[test]
#[serial]
fn test_pool_respects_env_var() {
    // Save original value if any
    let original = std::env::var("VK_PG_MAX_CONNECTIONS").ok();

    // SAFETY: We're modifying env vars in a controlled test environment.
    // These tests must be run with --test-threads=1 to avoid data races.
    unsafe {
        set_env("VK_PG_MAX_CONNECTIONS", "25");
    }

    let max_conns = get_max_connections();
    assert_eq!(max_conns, 25, "Should use value from VK_PG_MAX_CONNECTIONS");

    // Restore original value
    // SAFETY: Same as above
    unsafe {
        match original {
            Some(val) => set_env("VK_PG_MAX_CONNECTIONS", &val),
            None => remove_env("VK_PG_MAX_CONNECTIONS"),
        }
    }
}

/// Test that default is used when VK_PG_MAX_CONNECTIONS is not set.
#[test]
#[serial]
fn test_pool_default_when_no_env() {
    // Save original value if any
    let original = std::env::var("VK_PG_MAX_CONNECTIONS").ok();

    // SAFETY: We're modifying env vars in a controlled test environment.
    unsafe {
        remove_env("VK_PG_MAX_CONNECTIONS");
    }

    let max_conns = get_max_connections();
    assert_eq!(
        max_conns, 20,
        "Should use default of 20 when env var not set"
    );

    // Restore original value
    // SAFETY: Same as above
    unsafe {
        if let Some(val) = original {
            set_env("VK_PG_MAX_CONNECTIONS", &val);
        }
    }
}

/// Test that invalid values fall back to default.
#[test]
#[serial]
fn test_pool_invalid_env_uses_default() {
    // Save original value if any
    let original = std::env::var("VK_PG_MAX_CONNECTIONS").ok();

    // SAFETY: We're modifying env vars in a controlled test environment.
    unsafe {
        set_env("VK_PG_MAX_CONNECTIONS", "not_a_number");
    }

    let max_conns = get_max_connections();
    assert_eq!(
        max_conns, 20,
        "Should use default when env var is not a valid number"
    );

    // Restore original value
    // SAFETY: Same as above
    unsafe {
        match original {
            Some(val) => set_env("VK_PG_MAX_CONNECTIONS", &val),
            None => remove_env("VK_PG_MAX_CONNECTIONS"),
        }
    }
}

/// Test that negative values fall back to default.
#[test]
#[serial]
fn test_pool_negative_value_uses_default() {
    // Save original value if any
    let original = std::env::var("VK_PG_MAX_CONNECTIONS").ok();

    // SAFETY: We're modifying env vars in a controlled test environment.
    unsafe {
        set_env("VK_PG_MAX_CONNECTIONS", "-5");
    }

    let max_conns = get_max_connections();
    assert_eq!(max_conns, 20, "Should use default when env var is negative");

    // Restore original value
    // SAFETY: Same as above
    unsafe {
        match original {
            Some(val) => set_env("VK_PG_MAX_CONNECTIONS", &val),
            None => remove_env("VK_PG_MAX_CONNECTIONS"),
        }
    }
}

/// Test that zero value falls back to default (0 connections is not valid).
#[test]
#[serial]
fn test_pool_zero_value_uses_default() {
    // Save original value if any
    let original = std::env::var("VK_PG_MAX_CONNECTIONS").ok();

    // SAFETY: We're modifying env vars in a controlled test environment.
    unsafe {
        set_env("VK_PG_MAX_CONNECTIONS", "0");
    }

    let max_conns = get_max_connections();
    assert_eq!(max_conns, 20, "Should use default when env var is zero");

    // Restore original value
    // SAFETY: Same as above
    unsafe {
        match original {
            Some(val) => set_env("VK_PG_MAX_CONNECTIONS", &val),
            None => remove_env("VK_PG_MAX_CONNECTIONS"),
        }
    }
}
