//! Integration test harness for cqlsh-rs.
//!
//! All integration tests are compiled as a single test binary to share
//! a single ScyllaDB container instance across all tests.
//!
//! Run with: cargo test --test integration -- --ignored

mod core_tests;
mod escape_tests;
mod helpers;
mod output_tests;
mod unicode_tests;
