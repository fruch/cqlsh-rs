//! Shared test helpers for integration tests.
//!
//! Provides ScyllaDB container setup via testcontainers-rs and utility
//! functions for executing cqlsh-rs commands against a live database.

use std::sync::OnceLock;
use std::time::Duration;

use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::SyncRunner;
use testcontainers::{Container, GenericImage, ImageExt};

/// The native CQL transport port inside the container.
const CQL_PORT: u16 = 9042;

/// A running ScyllaDB container with its mapped port.
pub struct ScyllaContainer {
    /// The container handle — dropped when this struct is dropped.
    _container: Container<GenericImage>,
    /// The host port mapped to the CQL native transport port.
    pub port: u16,
    /// The host address (always 127.0.0.1 for local Docker).
    pub host: String,
}

/// Global singleton container to avoid spinning up a new one per test.
static SCYLLA: OnceLock<ScyllaContainer> = OnceLock::new();

/// Get or create the shared ScyllaDB test container.
///
/// The container is started once and reused across all integration tests.
/// Each test should create its own keyspace to avoid interference.
pub fn get_scylla() -> &'static ScyllaContainer {
    SCYLLA.get_or_init(|| {
        let image = GenericImage::new("scylladb/scylla", "6.2")
            .with_wait_for(WaitFor::message_on_stderr("serving"));

        let container = image
            .with_exposed_port(CQL_PORT.tcp())
            .with_cmd(vec![
                "--smp".to_string(),
                "1".to_string(),
                "--memory".to_string(),
                "512M".to_string(),
                "--overprovisioned".to_string(),
                "1".to_string(),
                "--skip-wait-for-gossip-to-settle".to_string(),
                "0".to_string(),
            ])
            .with_startup_timeout(Duration::from_secs(120))
            .start()
            .expect("failed to start ScyllaDB container");

        let port = container
            .get_host_port_ipv4(CQL_PORT)
            .expect("failed to get mapped port");

        let host = container
            .get_host()
            .expect("failed to get container host")
            .to_string();

        // Wait a bit for CQL to be fully ready after the log message
        std::thread::sleep(Duration::from_secs(5));

        ScyllaContainer {
            _container: container,
            port,
            host,
        }
    })
}

/// Execute cqlsh-rs with the given arguments against the test container.
///
/// Returns an `assert_cmd::Command` pre-configured with the container's
/// host and port.
pub fn cqlsh_cmd(scylla: &ScyllaContainer) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("cqlsh-rs").unwrap();
    cmd.args([&scylla.host, &scylla.port.to_string()]);
    cmd
}

/// Execute a CQL statement via cqlsh-rs `-e` flag and return the command assertion.
pub fn execute_cql(scylla: &ScyllaContainer, cql: &str) -> assert_cmd::assert::Assert {
    cqlsh_cmd(scylla).args(["-e", cql]).assert()
}

/// Execute a CQL statement and return stdout as a string.
/// Panics if the command fails.
pub fn execute_cql_output(scylla: &ScyllaContainer, cql: &str) -> String {
    let output = cqlsh_cmd(scylla)
        .args(["-e", cql])
        .output()
        .expect("failed to execute cqlsh-rs");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("cqlsh-rs failed: {stderr}");
    }

    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Create a unique test keyspace and return its name.
///
/// Uses SimpleStrategy with RF=1 for single-node test cluster.
pub fn create_test_keyspace(scylla: &ScyllaContainer, prefix: &str) -> String {
    let ks_name = format!(
        "test_{}_{:x}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 0xFFFFFF
    );

    execute_cql(
        scylla,
        &format!(
            "CREATE KEYSPACE IF NOT EXISTS {ks_name} \
             WITH replication = {{'class': 'SimpleStrategy', 'replication_factor': 1}}"
        ),
    )
    .success();

    ks_name
}

/// Drop a test keyspace (cleanup).
pub fn drop_test_keyspace(scylla: &ScyllaContainer, keyspace: &str) {
    execute_cql(scylla, &format!("DROP KEYSPACE IF EXISTS {keyspace}")).success();
}
