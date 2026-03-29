//! Integration tests for DESCRIBE extended commands (INDEX, MV, TYPE, FUNCTION, AGGREGATE).
//!
//! Corresponds to Phase 4 tasks 4.12–4.17.

use super::helpers::*;

#[test]
#[ignore = "requires Docker"]
fn test_describe_full_schema_includes_system() {
    let scylla = get_scylla();
    // Create a user keyspace so SCHEMA has something to show
    let ks = create_test_keyspace(scylla, "desc_full");

    let output = cqlsh_cmd(scylla)
        .args(["-e", "DESCRIBE FULL SCHEMA"])
        .output()
        .expect("failed to run cqlsh-rs");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // FULL SCHEMA must include system keyspaces unlike plain DESCRIBE SCHEMA
    assert!(
        stdout.contains("system"),
        "DESCRIBE FULL SCHEMA should include system keyspaces, got: {stdout}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_index() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_idx");

    execute_cql(
        scylla,
        &format!("CREATE TABLE {ks}.users (id int PRIMARY KEY, email text)"),
    )
    .success();
    execute_cql(
        scylla,
        &format!("CREATE INDEX email_idx ON {ks}.users (email)"),
    )
    .success();

    let output = execute_cql_output(scylla, &format!("DESCRIBE INDEX {ks}.email_idx"));
    assert!(
        output.contains("CREATE INDEX"),
        "DESCRIBE INDEX should show CREATE INDEX: {output}"
    );
    assert!(
        output.contains("email_idx"),
        "DESCRIBE INDEX should show index name: {output}"
    );
    assert!(
        output.contains("email"),
        "DESCRIBE INDEX should show indexed column: {output}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_materialized_view() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_mv");

    execute_cql(
        scylla,
        &format!("CREATE TABLE {ks}.users (id int PRIMARY KEY, email text)"),
    )
    .success();
    execute_cql(
        scylla,
        &format!(
            "CREATE MATERIALIZED VIEW {ks}.users_by_email AS \
             SELECT * FROM {ks}.users WHERE email IS NOT NULL AND id IS NOT NULL \
             PRIMARY KEY (email, id)"
        ),
    )
    .success();

    let output = execute_cql_output(
        scylla,
        &format!("DESCRIBE MATERIALIZED VIEW {ks}.users_by_email"),
    );
    assert!(
        output.contains("CREATE MATERIALIZED VIEW"),
        "DESCRIBE MATERIALIZED VIEW should show CREATE statement: {output}"
    );
    assert!(
        output.contains("users_by_email"),
        "DESCRIBE MATERIALIZED VIEW should show view name: {output}"
    );
    assert!(
        output.contains("PRIMARY KEY"),
        "DESCRIBE MATERIALIZED VIEW should show PRIMARY KEY: {output}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_type() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_type");

    execute_cql(
        scylla,
        &format!("CREATE TYPE {ks}.address (street text, city text, zip int)"),
    )
    .success();

    let output = execute_cql_output(scylla, &format!("DESCRIBE TYPE {ks}.address"));
    assert!(
        output.contains("CREATE TYPE"),
        "DESCRIBE TYPE should show CREATE TYPE: {output}"
    );
    assert!(
        output.contains("address"),
        "DESCRIBE TYPE should show type name: {output}"
    );
    assert!(
        output.contains("street"),
        "DESCRIBE TYPE should show field names: {output}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_types_list() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_types_list");

    execute_cql(
        scylla,
        &format!("CREATE TYPE {ks}.tag (name text, value text)"),
    )
    .success();

    // Use -k flag so DESCRIBE TYPES has a current keyspace
    let output = cqlsh_cmd(scylla)
        .args(["-k", &ks, "-e", "DESCRIBE TYPES"])
        .output()
        .expect("failed to run cqlsh-rs");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(
        stdout.contains("tag"),
        "DESCRIBE TYPES should list type names: {stdout}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_function() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_func");

    // ScyllaDB supports Lua UDFs; skip gracefully if UDFs are not enabled
    let create_result = cqlsh_cmd(scylla)
        .args([
            "-e",
            &format!(
                "CREATE OR REPLACE FUNCTION {ks}.double_val(val int) \
                 RETURNS NULL ON NULL INPUT \
                 RETURNS int \
                 LANGUAGE lua \
                 AS 'return val * 2';"
            ),
        ])
        .output()
        .expect("failed to run cqlsh-rs");

    if !create_result.status.success() {
        // UDFs not enabled on this instance — skip
        drop_test_keyspace(scylla, &ks);
        return;
    }

    let output = execute_cql_output(scylla, &format!("DESCRIBE FUNCTION {ks}.double_val"));
    assert!(
        output.contains("CREATE OR REPLACE FUNCTION"),
        "DESCRIBE FUNCTION should show CREATE FUNCTION: {output}"
    );
    assert!(
        output.contains("double_val"),
        "DESCRIBE FUNCTION should show function name: {output}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_aggregate() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_agg");

    // Create state function first; skip if UDFs/UDAs not enabled
    let create_func = cqlsh_cmd(scylla)
        .args([
            "-e",
            &format!(
                "CREATE OR REPLACE FUNCTION {ks}.sum_state(state int, val int) \
                 CALLED ON NULL INPUT \
                 RETURNS int \
                 LANGUAGE lua \
                 AS 'return state + val';"
            ),
        ])
        .output()
        .expect("failed to run cqlsh-rs");

    if !create_func.status.success() {
        drop_test_keyspace(scylla, &ks);
        return;
    }

    execute_cql(
        scylla,
        &format!(
            "CREATE OR REPLACE AGGREGATE {ks}.my_sum(int) \
             SFUNC sum_state \
             STYPE int \
             INITCOND 0"
        ),
    )
    .success();

    let output = execute_cql_output(scylla, &format!("DESCRIBE AGGREGATE {ks}.my_sum"));
    assert!(
        output.contains("CREATE OR REPLACE AGGREGATE"),
        "DESCRIBE AGGREGATE should show CREATE AGGREGATE: {output}"
    );
    assert!(
        output.contains("my_sum"),
        "DESCRIBE AGGREGATE should show aggregate name: {output}"
    );
    assert!(
        output.contains("SFUNC"),
        "DESCRIBE AGGREGATE should show SFUNC: {output}"
    );

    drop_test_keyspace(scylla, &ks);
}

#[test]
#[ignore = "requires Docker"]
fn test_describe_nonexistent_index() {
    let scylla = get_scylla();
    let ks = create_test_keyspace(scylla, "desc_noexist");

    // Should exit 0 and print a "not found" message (not crash)
    let output = cqlsh_cmd(scylla)
        .args(["-e", &format!("DESCRIBE INDEX {ks}.no_such_idx")])
        .output()
        .expect("failed to run cqlsh-rs");

    assert!(
        output.status.success(),
        "DESCRIBE of non-existent index should not fail with non-zero exit"
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.to_lowercase().contains("not found"),
        "Should print 'not found' message: {combined}"
    );

    drop_test_keyspace(scylla, &ks);
}
