//! Property-based tests for cqlsh-rs using `proptest`.
//!
//! Covers:
//!   1. Parser no-panic guarantee (arbitrary input never panics)
//!   2. CSV value formatting determinism (same input → same output)
//!   3. Config parse determinism (same INI content → consistent Ok/Err)
//!   4. CSV roundtrip (values written to CSV and read back have the same field count)

use cqlsh_rs::{
    config::CqlshrcConfig,
    copy::{format_value_for_csv, CopyOptions},
    driver::types::CqlValue,
    parser::StatementParser,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// CqlValue strategy — a subset of simple, non-recursive variants
// ---------------------------------------------------------------------------

fn arb_simple_cql_value() -> impl Strategy<Value = CqlValue> {
    prop_oneof![
        any::<i32>().prop_map(CqlValue::Int),
        any::<i64>().prop_map(CqlValue::BigInt),
        any::<i16>().prop_map(CqlValue::SmallInt),
        any::<i8>().prop_map(CqlValue::TinyInt),
        any::<bool>().prop_map(CqlValue::Boolean),
        // Restrict to finite floats so CSV output is well-defined
        any::<f32>()
            .prop_filter("must be finite", |f| f.is_finite())
            .prop_map(CqlValue::Float),
        any::<f64>()
            .prop_filter("must be finite", |f| f.is_finite())
            .prop_map(CqlValue::Double),
        "[a-zA-Z0-9 ]{0,50}".prop_map(CqlValue::Text),
        Just(CqlValue::Null),
    ]
}

// ---------------------------------------------------------------------------
// 1. Parser no-panic guarantee
// ---------------------------------------------------------------------------

proptest! {
    /// Feeding arbitrary Unicode lines to StatementParser never panics.
    #[test]
    fn parser_never_panics(s in ".*") {
        let mut p = StatementParser::new();
        let _ = p.feed_line(&s);
    }

    /// Feeding a word followed by a semicolon always yields a Complete result.
    #[test]
    fn parser_word_plus_semicolon_is_complete(word in "[A-Za-z][A-Za-z0-9_]{0,20}") {
        use cqlsh_rs::parser::ParseResult;
        let mut p = StatementParser::new();
        let stmt = format!("{word};");
        let result = p.feed_line(&stmt);
        prop_assert!(
            matches!(result, ParseResult::Complete(_)),
            "expected Complete for '{stmt}', got Incomplete"
        );
    }

    /// Resetting the parser always leaves it in the empty state.
    #[test]
    fn parser_reset_is_idempotent(s in "[A-Za-z0-9 ]{0,40}") {
        let mut p = StatementParser::new();
        let _ = p.feed_line(&s);
        p.reset();
        prop_assert!(p.is_empty());
        p.reset();
        prop_assert!(p.is_empty());
    }
}

// ---------------------------------------------------------------------------
// 2. CSV value formatting determinism
// ---------------------------------------------------------------------------

proptest! {
    /// format_value_for_csv produces the same string on repeated calls.
    #[test]
    fn format_csv_is_deterministic(v in arb_simple_cql_value()) {
        let opts = CopyOptions::default();
        let s1 = format_value_for_csv(&v, &opts);
        let s2 = format_value_for_csv(&v, &opts);
        prop_assert_eq!(s1, s2);
    }

    /// Cloning a value and formatting both clones yields the same string.
    #[test]
    fn format_csv_clone_equal(v in arb_simple_cql_value()) {
        let opts = CopyOptions::default();
        let v2 = v.clone();
        prop_assert_eq!(
            format_value_for_csv(&v, &opts),
            format_value_for_csv(&v2, &opts)
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Config parse determinism
// ---------------------------------------------------------------------------

proptest! {
    /// Parsing the same INI content twice always gives the same Ok/Err outcome.
    #[test]
    fn config_parse_deterministic(content in "[a-zA-Z0-9 =\n\r._-]{0,200}") {
        let r1 = CqlshrcConfig::parse(&content);
        let r2 = CqlshrcConfig::parse(&content);
        prop_assert_eq!(
            r1.is_ok(),
            r2.is_ok(),
            "inconsistent Ok/Err for same config content"
        );
    }

    /// Valid [connection] INI sections always parse successfully.
    #[test]
    fn config_valid_connection_section_parses(
        host in "[a-z]{1,20}",
        port in 1024u16..=65535u16,
    ) {
        let content = format!("[connection]\nhostname = {host}\nport = {port}\n");
        prop_assert!(
            CqlshrcConfig::parse(&content).is_ok(),
            "expected Ok for: {content}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. CSV roundtrip: write values, read back, verify field count
// ---------------------------------------------------------------------------

proptest! {
    /// Writing CQL values as a CSV row and reading it back preserves field count.
    #[test]
    fn csv_roundtrip_field_count(
        values in prop::collection::vec(arb_simple_cql_value(), 1..=10)
    ) {
        let opts = CopyOptions::default();
        let fields: Vec<String> = values.iter().map(|v| format_value_for_csv(v, &opts)).collect();

        // Write as a CSV record
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(vec![]);
        wtr.write_record(&fields).unwrap();
        let data = wtr.into_inner().unwrap();

        // Read back
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(data.as_slice());
        let record = rdr.records().next().unwrap().unwrap();

        prop_assert_eq!(
            record.len(),
            values.len(),
            "field count mismatch after CSV roundtrip"
        );
    }

    /// CSV roundtrip: Null values come back as the configured null_val string.
    #[test]
    fn csv_roundtrip_null_preserved(null_val in "[A-Z]{0,10}") {
        let opts = CopyOptions {
            null_val: null_val.clone(),
            ..CopyOptions::default()
        };
        let formatted = format_value_for_csv(&CqlValue::Null, &opts);
        prop_assert_eq!(formatted, null_val);
    }
}
