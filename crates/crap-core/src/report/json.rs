//! Versioned JSON envelope for automation.

use super::display_path;
use crate::error::{Error, Result};
use crate::merge::{CrapEntry, ambiguous_join_count};
use crate::metric::Metric;
use crate::score::{Risk, classify_risk, exceeds_threshold, to_f64};
use serde::Serialize;

/// Builds a schema-versioned JSON document for `entries`.
///
/// # Errors
///
/// Returns [`Error::Report`] if the document cannot be serialized.
pub fn render_json(
    entries: &[CrapEntry],
    threshold: f64,
    metric: Metric,
    gate_failed: bool,
    language: &str,
) -> Result<String> {
    let exceeding = entries.iter().filter(|e| exceeds_threshold(e.crap, threshold)).count();
    let scores: Vec<f64> = entries.iter().map(|e| e.crap).collect();
    let doc = ReportDoc {
        schema_version: 3,
        language,
        metric: metric.to_string(),
        threshold,
        result: ResultDoc {
            passed: exceeding == 0,
            gate_failed,
            summary: SummaryDoc {
                functions: entries.len(),
                exceeding,
                ambiguous: ambiguous_join_count(entries),
                average_crap: average(&scores),
                median_crap: median(&scores),
                risk: risk_counts(entries),
            },
            functions: entries.iter().map(|e| function_doc(e, threshold)).collect(),
        },
    };
    stringify_json(&doc)
}

fn stringify_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|err| Error::report(format!("json report: {err}")))
}

#[derive(Serialize)]
struct ReportDoc<'a> {
    schema_version: u32,
    language: &'a str,
    metric: String,
    threshold: f64,
    result: ResultDoc,
}

#[derive(Serialize)]
struct ResultDoc {
    passed: bool,
    gate_failed: bool,
    summary: SummaryDoc,
    functions: Vec<FunctionDoc>,
}

#[derive(Serialize)]
struct SummaryDoc {
    functions: usize,
    exceeding: usize,
    ambiguous: usize,
    average_crap: f64,
    median_crap: f64,
    risk: RiskCounts,
}

#[derive(Serialize, Default)]
struct RiskCounts {
    low: usize,
    acceptable: usize,
    moderate: usize,
    high: usize,
}

#[derive(Serialize)]
struct FunctionDoc {
    exceeds: bool,
    identity: IdentityDoc,
    complexity: usize,
    coverage_percent: f64,
    coverage_join: &'static str,
    crap: f64,
    risk: String,
}

#[derive(Serialize)]
struct IdentityDoc {
    file: String,
    function: String,
    #[serde(rename = "crate")]
    crate_name: Option<String>,
    span: SpanDoc,
}

#[derive(Serialize)]
struct SpanDoc {
    start_line: usize,
    end_line: usize,
}

fn function_doc(entry: &CrapEntry, threshold: f64) -> FunctionDoc {
    FunctionDoc {
        exceeds: exceeds_threshold(entry.crap, threshold),
        identity: IdentityDoc {
            file: display_path(&entry.file),
            function: entry.function.clone(),
            crate_name: entry.crate_name.clone(),
            span: SpanDoc {
                start_line: entry.start_line,
                end_line: entry.end_line,
            },
        },
        complexity: entry.complexity,
        coverage_percent: entry.coverage,
        coverage_join: entry.coverage_join.as_str(),
        crap: entry.crap,
        risk: classify_risk(entry.crap).to_string(),
    }
}

fn risk_counts(entries: &[CrapEntry]) -> RiskCounts {
    let mut counts = RiskCounts::default();
    for entry in entries {
        increment_risk(&mut counts, classify_risk(entry.crap));
    }
    counts
}

const fn increment_risk(counts: &mut RiskCounts, risk: Risk) {
    match risk {
        Risk::Low => counts.low += 1,
        Risk::Acceptable => counts.acceptable += 1,
        Risk::Moderate => counts.moderate += 1,
        Risk::High => counts.high += 1,
    }
}

fn average(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / to_f64(scores.len())
    }
}

fn median(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let mut ordered = scores.to_vec();
    ordered.sort_by(f64::total_cmp);
    let mid = ordered.len() / 2;
    if ordered.len() % 2 == 1 {
        ordered[mid]
    } else {
        f64::midpoint(ordered[mid - 1], ordered[mid])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::CoverageJoin;
    use crate::metric::Metric;
    use serde_json::Value;
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
            entry("okfn", 1.0, 1, 100.0, Some("demo"), 12),
            entry("crappy", 156.0, 12, 0.0, Some("demo"), 24),
        ]
    }

    fn assert_fields(value: &Value, fields: &[(&str, Value)]) {
        for (ptr, want) in fields {
            let got = value.pointer(ptr).cloned().unwrap_or(Value::Null);
            assert_eq!(got, *want, "{ptr}");
        }
    }

    #[test]
    fn envelope_header_and_summary() {
        let value = json(&mixed_entries(), 15.0, Metric::Cyclomatic, false);
        assert_fields(
            &value,
            &[
                ("/schema_version", Value::from(3)),
                ("/language", Value::from("rust")),
                ("/metric", Value::from("cyclomatic")),
                ("/threshold", Value::from(15.0)),
                ("/result/passed", Value::from(false)),
                ("/result/gate_failed", Value::from(false)),
                ("/result/summary/functions", Value::from(2)),
                ("/result/summary/exceeding", Value::from(1)),
                ("/result/summary/ambiguous", Value::from(0)),
                ("/result/summary/average_crap", Value::from(78.5)),
                ("/result/summary/median_crap", Value::from(78.5)),
                ("/result/summary/risk/low", Value::from(1)),
                ("/result/summary/risk/high", Value::from(1)),
            ],
        );
    }

    #[test]
    fn envelope_function_axes_are_independent() {
        let value = json(&mixed_entries(), 15.0, Metric::Cyclomatic, false);
        assert_fields(
            &value,
            &[
                ("/result/functions/1/exceeds", Value::from(true)),
                ("/result/functions/1/risk", Value::from("high")),
                ("/result/functions/1/coverage_join", Value::from("measured")),
                (
                    "/result/functions/1/identity/span/end_line",
                    Value::from(24),
                ),
                ("/result/functions/0/exceeds", Value::from(false)),
                ("/result/functions/0/risk", Value::from("low")),
                ("/result/functions/0/coverage_join", Value::from("measured")),
            ],
        );
    }

    #[test]
    fn missing_crate_is_null() {
        let entries = [entry("anon", 1.0, 1, 100.0, None, 11)];
        let value = json(&entries, 15.0, Metric::Cognitive, false);
        assert_eq!(value["metric"], "cognitive");
        assert!(value["result"]["functions"][0]["identity"]["crate"].is_null());
    }

    #[test]
    fn empty_run_zeros_summary() {
        let value = json(&[], 15.0, Metric::Cyclomatic, false);
        assert_fields(
            &value,
            &[
                ("/result/passed", Value::from(true)),
                ("/result/summary/functions", Value::from(0)),
                ("/result/summary/exceeding", Value::from(0)),
                ("/result/summary/ambiguous", Value::from(0)),
                ("/result/summary/average_crap", Value::from(0.0)),
                ("/result/summary/median_crap", Value::from(0.0)),
                ("/result/summary/risk/low", Value::from(0)),
            ],
        );
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
        assert_eq!(value["result"]["passed"], true);
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
        assert_eq!(value["result"]["passed"], true);
        assert_eq!(value["result"]["gate_failed"], false);
    }

    #[test]
    fn passed_tracks_exceedances_not_the_gate_flag() {
        let value = json(&mixed_entries(), 15.0, Metric::Cyclomatic, true);
        assert_eq!(value["result"]["passed"], false);
        assert_eq!(value["result"]["gate_failed"], true);
        assert_eq!(value["result"]["summary"]["exceeding"], 1);
    }

    #[test]
    fn ambiguous_join_is_reported_in_json() {
        let mut entries = mixed_entries();
        entries[1].coverage_join = CoverageJoin::Ambiguous;
        let value = json(&entries, 15.0, Metric::Cyclomatic, false);
        assert_eq!(value["result"]["summary"]["ambiguous"], 1);
        assert_eq!(
            value["result"]["functions"][1]["coverage_join"],
            "ambiguous"
        );
    }

    #[test]
    fn language_is_taken_from_the_caller() {
        let value = json_lang(&[], 15.0, Metric::Cyclomatic, false, "demo");
        assert_eq!(value["language"], "demo");
    }

    #[test]
    fn stringify_json_rejects_compound_map_keys() {
        let mut map = std::collections::BTreeMap::new();
        map.insert((1_u32, 2_u32), 3_u32);
        let text = stringify_json(&map);
        assert!(matches!(text, Err(Error::Report(_))), "{text:?}");
    }
}
