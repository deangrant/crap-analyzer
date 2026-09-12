//! Unit tests for the JSON report writer.

use super::*;
use crate::merge::CoverageJoin;
use crate::metric::Metric;
use serde_json::Value;
use std::io::{self, Write};
use std::path::PathBuf;

fn entry(
    name: &str,
    crap: f64,
    cc: usize,
    cov: f64,
    crate_name: Option<&str>,
    end_line: usize,
) -> CrapEntry {
    CrapEntry {
        file: PathBuf::from("src/lib.rs"),
        function: name.into(),
        start_line: 10,
        end_line,
        complexity: cc,
        coverage: cov,
        coverage_join: CoverageJoin::Measured,
        crap,
        crate_name: crate_name.map(str::to_owned),
    }
}

fn parse(text: &str) -> Value {
    let parsed = serde_json::from_str(text);
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or(Value::Null)
}

fn json(entries: &[CrapEntry], threshold: f64, metric: Metric, gate_failed: bool) -> Value {
    json_lang(entries, threshold, metric, gate_failed, "rust")
}

fn json_lang(
    entries: &[CrapEntry],
    threshold: f64,
    metric: Metric,
    gate_failed: bool,
    language: &str,
) -> Value {
    let text = render_json(entries, threshold, metric, gate_failed, language);
    assert!(text.is_ok(), "{text:?}");
    parse(&text.unwrap_or_default())
}

fn mixed_entries() -> [CrapEntry; 2] {
    [
        entry("crappy", 156.0, 12, 0.0, Some("demo"), 40),
        entry("okfn", 1.0, 1, 100.0, Some("demo"), 12),
    ]
}

#[test]
fn schema_lists_risk_and_gate_fields() {
    let value = json(&mixed_entries(), 15.0, Metric::Cyclomatic, false);
    for (pointer, want) in [
        ("/schema_version", Value::from(4)),
        ("/language", Value::from("rust")),
        ("/metric", Value::from("cyclomatic")),
        ("/threshold", Value::from(15.0)),
        ("/result/threshold_cleared", Value::from(false)),
        ("/result/gate_failed", Value::from(false)),
        ("/result/summary/functions", Value::from(2)),
        ("/result/summary/exceeding", Value::from(1)),
        ("/result/summary/ambiguous", Value::from(0)),
        ("/result/summary/risk/high", Value::from(1)),
        ("/result/summary/risk/low", Value::from(1)),
    ] {
        assert_eq!(value.pointer(pointer), Some(&want), "{pointer}");
    }
    assert_eq!(
        value["result"]["functions"][0]["identity"]["function"],
        "crappy"
    );
    assert_eq!(value["result"]["functions"][0]["exceeds"], true);
    assert_eq!(value["result"]["functions"][0]["coverage_join"], "measured");
}

#[test]
fn ambiguous_join_is_counted_and_labeled() {
    let mut entries = mixed_entries();
    entries[0].coverage_join = CoverageJoin::Ambiguous;
    let value = json(&entries, 15.0, Metric::Cyclomatic, false);
    assert_eq!(value["result"]["summary"]["ambiguous"], 1);
    assert_eq!(
        value["result"]["functions"][0]["coverage_join"],
        "ambiguous"
    );
}

#[test]
fn language_and_cognitive_metric_are_emitted() {
    let entries = [entry("okfn", 1.0, 1, 100.0, None, 1)];
    let value = json_lang(&entries, 15.0, Metric::Cognitive, false, "go");
    assert_eq!(value["language"], "go");
    assert_eq!(value["metric"], "cognitive");
    assert_eq!(value["result"]["threshold_cleared"], true);
}

#[test]
fn empty_entries_zero_averages() {
    let value = json(&[], 15.0, Metric::Cyclomatic, false);
    assert_eq!(value["result"]["summary"]["functions"], 0);
    assert_eq!(value["result"]["summary"]["average_crap"], 0.0);
    assert_eq!(value["result"]["summary"]["median_crap"], 0.0);
    assert!(value["result"]["functions"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn acceptable_risk_is_counted() {
    let entries = [entry("mid", 10.0, 5, 80.0, Some("demo"), 20)];
    let value = json(&entries, 15.0, Metric::Cyclomatic, false);
    assert_eq!(value["result"]["summary"]["risk"]["acceptable"], 1);
    assert_eq!(value["result"]["functions"][0]["risk"], "acceptable");
    assert_eq!(value["result"]["functions"][0]["exceeds"], false);
}

#[test]
fn moderate_does_not_imply_exceeds() {
    let entries = [entry("mid", 20.0, 10, 50.0, Some("demo"), 20)];
    let value = json(&entries, 25.0, Metric::Cyclomatic, false);
    assert_eq!(value["result"]["threshold_cleared"], true);
    assert_eq!(value["result"]["functions"][0]["exceeds"], false);
    assert_eq!(value["result"]["functions"][0]["risk"], "moderate");
}

#[test]
fn even_count_median_is_the_midpoint() {
    let entries = [
        entry("a", 2.0, 1, 100.0, None, 1),
        entry("b", 4.0, 1, 100.0, None, 1),
        entry("c", 6.0, 1, 100.0, None, 1),
        entry("d", 8.0, 1, 100.0, None, 1),
    ];
    let value = json(&entries, 15.0, Metric::Cyclomatic, false);
    assert_eq!(value["result"]["summary"]["median_crap"], 5.0);
    assert_eq!(value["result"]["threshold_cleared"], true);
    assert_eq!(value["result"]["gate_failed"], false);
}

#[test]
fn threshold_cleared_tracks_exceedances_not_the_gate_flag() {
    let value = json(&mixed_entries(), 15.0, Metric::Cyclomatic, true);
    assert_eq!(value["result"]["threshold_cleared"], false);
    assert_eq!(value["result"]["gate_failed"], true);
}

#[test]
fn write_json_matches_render_json_parse() {
    let entries = mixed_entries();
    let rendered = render_json(&entries, 15.0, Metric::Cyclomatic, false, "rust");
    assert!(rendered.is_ok(), "{rendered:?}");
    let mut buf = Vec::new();
    let written = write_json(&mut buf, &entries, 15.0, Metric::Cyclomatic, false, "rust");
    assert!(written.is_ok(), "{written:?}");
    let from_render = parse(&rendered.unwrap_or_default());
    let from_write = parse(&String::from_utf8(buf).unwrap_or_default());
    assert_eq!(from_render, from_write);
}

#[test]
fn write_json_maps_io_errors() {
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("boom"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut fail = Fail;
    let err = write_json(&mut fail, &[], 15.0, Metric::Cyclomatic, false, "rust");
    assert!(err.is_err(), "{err:?}");
    assert!(fail.flush().is_ok());
}

#[test]
fn finish_json_buffer_maps_write_errors() {
    let err = finish_json_buffer(Err(Error::report("boom")), Vec::new());
    assert!(err.is_err(), "{err:?}");
    assert!(finish_json_buffer(Ok(()), b"{}".to_vec()).is_ok());
}

#[test]
fn write_json_maps_mid_stream_io_errors() {
    struct FailAfter {
        ok_writes: usize,
        seen: usize,
    }
    impl Write for FailAfter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.seen >= self.ok_writes {
                return Err(io::Error::other("boom"));
            }
            self.seen += 1;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Count(usize);
    impl Write for Count {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct FailOnSep;
    impl Write for FailOnSep {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if buf == b"\n" || buf == b",\n" {
                return Err(io::Error::other("sep"));
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let entries = mixed_entries();
    let mut count = Count(0);
    assert!(
        write_json(
            &mut count,
            &entries,
            15.0,
            Metric::Cyclomatic,
            false,
            "rust"
        )
        .is_ok()
    );
    let total = count.0;
    assert!(total > 2);
    let mut saw_err = false;
    for ok_writes in [1_usize, total / 2, total.saturating_sub(1)] {
        let mut writer = FailAfter { ok_writes, seen: 0 };
        saw_err |= write_json(
            &mut writer,
            &entries,
            15.0,
            Metric::Cyclomatic,
            false,
            "rust",
        )
        .is_err();
        assert!(writer.flush().is_ok());
    }
    assert!(saw_err);
    let mut sep_fail = FailOnSep;
    assert!(
        write_json(
            &mut sep_fail,
            &entries,
            15.0,
            Metric::Cyclomatic,
            false,
            "rust",
        )
        .is_err()
    );
    assert!(sep_fail.flush().is_ok());
}

#[test]
fn pretty_text_from_uses_fallback_on_err() {
    let err = serde_json::Error::io(io::Error::other("boom"));
    assert_eq!(pretty_text_from(Err(err)), "");
    assert!(!pretty_text(&function_doc(&mixed_entries()[0], 15.0)).is_empty());
}

#[test]
fn utf8_json_maps_invalid_bytes() {
    assert!(utf8_json(vec![0xff]).is_err());
    assert_eq!(utf8_json(b"{}".to_vec()).unwrap_or_default(), "{}");
}

#[test]
fn serde_string_from_uses_fallback_on_err() {
    let err = serde_json::Error::io(io::Error::other("boom"));
    assert_eq!(serde_string_from(Err(err), "0.0"), "0.0");
    assert_eq!(json_str("rust"), "\"rust\"");
    assert_eq!(json_number(1.5), "1.5");
}
