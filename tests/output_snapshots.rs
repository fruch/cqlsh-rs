//! Snapshot tests for cqlsh-rs output formatting.
//!
//! Covers tabular, expanded, JSON, error message, and HELP outputs using `insta`.
//! Run `cargo insta review` after the first run to approve the generated snapshots.

use cqlsh_rs::{
    colorizer::CqlColorizer,
    driver::types::{CqlColumn, CqlResult, CqlRow, CqlValue},
    error::format_error,
    formatter::{print_expanded, print_json, print_tabular},
    repl::{print_help, print_help_topic},
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn no_color() -> CqlColorizer {
    CqlColorizer::new(false)
}

fn capture_tabular(result: &CqlResult) -> String {
    let mut buf = Vec::new();
    print_tabular(result, &no_color(), &mut buf);
    String::from_utf8(buf).unwrap()
}

fn capture_expanded(result: &CqlResult) -> String {
    let mut buf = Vec::new();
    print_expanded(result, &no_color(), &mut buf);
    String::from_utf8(buf).unwrap()
}

fn capture_json(result: &CqlResult) -> String {
    let mut buf = Vec::new();
    print_json(result, &mut buf);
    String::from_utf8(buf).unwrap()
}

/// Build a simple two-column (int id, text name) result with `n` rows.
fn make_id_name_result(n: usize) -> CqlResult {
    CqlResult {
        columns: vec![
            CqlColumn {
                name: "id".to_string(),
                type_name: "int".to_string(),
            },
            CqlColumn {
                name: "name".to_string(),
                type_name: "text".to_string(),
            },
        ],
        rows: (1..=n)
            .map(|i| CqlRow {
                values: vec![CqlValue::Int(i as i32), CqlValue::Text(format!("User{i}"))],
            })
            .collect(),
        has_rows: true,
        tracing_id: None,
        warnings: vec![],
    }
}

/// Build a mixed-types result (10 rows) with text, int, float, boolean, uuid, null.
fn make_mixed_result(n: usize) -> CqlResult {
    let nil_uuid = Uuid::nil();
    CqlResult {
        columns: vec![
            CqlColumn {
                name: "id".to_string(),
                type_name: "int".to_string(),
            },
            CqlColumn {
                name: "label".to_string(),
                type_name: "text".to_string(),
            },
            CqlColumn {
                name: "score".to_string(),
                type_name: "float".to_string(),
            },
            CqlColumn {
                name: "active".to_string(),
                type_name: "boolean".to_string(),
            },
            CqlColumn {
                name: "token".to_string(),
                type_name: "uuid".to_string(),
            },
            CqlColumn {
                name: "note".to_string(),
                type_name: "text".to_string(),
            },
        ],
        rows: (1..=n)
            .map(|i| CqlRow {
                values: vec![
                    CqlValue::Int(i as i32),
                    CqlValue::Text(format!("label-{i}")),
                    CqlValue::Float(i as f32 * 1.5),
                    CqlValue::Boolean(i % 2 == 0),
                    CqlValue::Uuid(nil_uuid),
                    if i % 3 == 0 {
                        CqlValue::Null
                    } else {
                        CqlValue::Text(format!("note-{i}"))
                    },
                ],
            })
            .collect(),
        has_rows: true,
        tracing_id: None,
        warnings: vec![],
    }
}

// ---------------------------------------------------------------------------
// Tabular output snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_tabular_5_rows() {
    let result = make_id_name_result(5);
    insta::assert_snapshot!(capture_tabular(&result));
}

#[test]
fn snapshot_tabular_10_rows_mixed_types() {
    let result = make_mixed_result(10);
    insta::assert_snapshot!(capture_tabular(&result));
}

#[test]
fn snapshot_tabular_50_rows() {
    let result = make_id_name_result(50);
    insta::assert_snapshot!(capture_tabular(&result));
}

// ---------------------------------------------------------------------------
// Expanded output snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_expanded_5_rows() {
    let result = make_id_name_result(5);
    insta::assert_snapshot!(capture_expanded(&result));
}

#[test]
fn snapshot_expanded_mixed_types() {
    let result = make_mixed_result(5);
    insta::assert_snapshot!(capture_expanded(&result));
}

// ---------------------------------------------------------------------------
// JSON output snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_json_5_rows() {
    let result = make_id_name_result(5);
    insta::assert_snapshot!(capture_json(&result));
}

#[test]
fn snapshot_json_mixed_types() {
    let result = make_mixed_result(5);
    insta::assert_snapshot!(capture_json(&result));
}

// ---------------------------------------------------------------------------
// Error message formatting snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_error_syntax_exception() {
    use scylla::errors::DbError;
    use scylla::errors::{ExecutionError, RequestAttemptError};
    let attempt = RequestAttemptError::DbError(
        DbError::SyntaxError,
        "Error message: line 1:0 no viable alternative at input 'SELEC'".to_string(),
    );
    let err = anyhow::Error::new(ExecutionError::LastAttemptError(attempt));
    insta::assert_snapshot!(format_error(&err));
}

#[test]
fn snapshot_error_invalid_request() {
    use scylla::errors::DbError;
    use scylla::errors::{ExecutionError, RequestAttemptError};
    let attempt = RequestAttemptError::DbError(
        DbError::Invalid,
        "Error message: unconfigured table no_such_table".to_string(),
    );
    let err = anyhow::Error::new(ExecutionError::LastAttemptError(attempt));
    insta::assert_snapshot!(format_error(&err));
}

#[test]
fn snapshot_error_unauthorized() {
    use scylla::errors::DbError;
    use scylla::errors::{ExecutionError, RequestAttemptError};
    let attempt = RequestAttemptError::DbError(
        DbError::Unauthorized,
        "User anonymous has no SELECT permission on table system.users".to_string(),
    );
    let err = anyhow::Error::new(ExecutionError::LastAttemptError(attempt));
    insta::assert_snapshot!(format_error(&err));
}

#[test]
fn snapshot_error_fallback() {
    let err = anyhow::anyhow!("table foo does not exist");
    insta::assert_snapshot!(format_error(&err));
}

// ---------------------------------------------------------------------------
// HELP output snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_help_output() {
    let mut buf = Vec::new();
    print_help(&mut buf);
    insta::assert_snapshot!(String::from_utf8(buf).unwrap());
}

#[test]
fn snapshot_help_topic_consistency() {
    let mut buf = Vec::new();
    print_help_topic("CONSISTENCY", &mut buf);
    insta::assert_snapshot!(String::from_utf8(buf).unwrap());
}

#[test]
fn snapshot_help_topic_unknown() {
    let mut buf = Vec::new();
    print_help_topic("FOOBAR", &mut buf);
    insta::assert_snapshot!(String::from_utf8(buf).unwrap());
}
