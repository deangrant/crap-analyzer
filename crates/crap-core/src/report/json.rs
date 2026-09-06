//! Versioned JSON envelope for automation.

use super::display_path;
use crate::merge::CrapEntry;
use crate::metric::Metric;
use crate::score::{Risk, classify_risk, exceeds_threshold};
use serde::Serialize;

/// Builds a schema-versioned JSON document for `entries`.
#[must_use]
pub fn render_json(
    entries: &[CrapEntry],
    threshold: f64,
    metric: Metric,
    gate_failed: bool,
) -> String {
    let exceeding = entries.iter().filter(|e| exceeds_threshold(e.crap, threshold)).count();
    let scores: Vec<f64> = entries.iter().map(|e| e.crap).collect();
    let doc = ReportDoc {
        schema_version: 1,
        language: "rust",
        metric: metric.to_string(),
        threshold,
        result: ResultDoc {
            passed: exceeding == 0,
            gate_failed,
            summary: SummaryDoc {
                functions: entries.len(),
                exceeding,
                average_crap: average(&scores),
                median_crap: median(&scores),
                risk: risk_counts(entries),
            },
            functions: entries.iter().map(|e| function_doc(e, threshold)).collect(),
        },
    };
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into())
}

#[derive(Serialize)]
struct ReportDoc {
    schema_version: u32,
    language: &'static str,
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
                start_line: entry.line,
                end_line: entry.end_line,
            },
        },
        complexity: entry.complexity,
        coverage_percent: entry.coverage,
        crap: entry.crap,
        risk: classify_risk(entry.crap).to_string(),
    }
}

fn risk_counts(entries: &[CrapEntry]) -> RiskCounts {
    let mut counts = RiskCounts::default();
    for entry in entries {
        match classify_risk(entry.crap) {
            Risk::Low => counts.low += 1,
            Risk::Acceptable => counts.acceptable += 1,
            Risk::Moderate => counts.moderate += 1,
            Risk::High => counts.high += 1,
        }
    }
    counts
}

fn average(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
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
            line: 10,
            end_line,
            complexity: cc,
            coverage: cov,
            crap,
            crate_name: crate_name.map(str::to_owned),
        }
    }

    fn parse(text: &str) -> Value {
        let parsed = serde_json::from_str(text);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or(Value::Null)
    }

    fn mixed_entries() -> [CrapEntry; 2] {
        [
            entry("okfn", 1.0, 1, 100.0, Some("demo"), 12),
            entry("crappy", 156.0, 12, 0.0, Some("demo"), 24),
        ]
    }

    #[test]
    fn envelope_header_and_summary() {
        let value = parse(&render_json(
            &mixed_entries(),
            15.0,
            Metric::Cyclomatic,
            false,
        ));
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["language"], "rust");
        assert_eq!(value["metric"], "cyclomatic");
        assert_eq!(value["threshold"], 15.0);
        assert_eq!(value["result"]["passed"], false);
        assert_eq!(value["result"]["gate_failed"], false);
        assert_eq!(value["result"]["summary"]["functions"], 2);
        assert_eq!(value["result"]["summary"]["exceeding"], 1);
        assert_eq!(value["result"]["summary"]["average_crap"], 78.5);
        assert_eq!(value["result"]["summary"]["median_crap"], 78.5);
        assert_eq!(value["result"]["summary"]["risk"]["low"], 1);
        assert_eq!(value["result"]["summary"]["risk"]["high"], 1);
    }

    #[test]
    fn envelope_function_axes_are_independent() {
        let value = parse(&render_json(
            &mixed_entries(),
            15.0,
            Metric::Cyclomatic,
            false,
        ));
        let funcs = &value["result"]["functions"];
        assert_eq!(funcs[1]["exceeds"], true);
        assert_eq!(funcs[1]["risk"], "high");
        assert_eq!(funcs[1]["identity"]["span"]["end_line"], 24);
        assert_eq!(funcs[0]["exceeds"], false);
        assert_eq!(funcs[0]["risk"], "low");
    }

    #[test]
    fn missing_crate_is_null() {
        let entries = [entry("anon", 1.0, 1, 100.0, None, 11)];
        let value = parse(&render_json(&entries, 15.0, Metric::Cognitive, false));
        assert_eq!(value["metric"], "cognitive");
        assert!(value["result"]["functions"][0]["identity"]["crate"].is_null());
    }

    #[test]
    fn empty_run_zeros_summary() {
        let value = parse(&render_json(&[], 15.0, Metric::Cyclomatic, false));
        assert_eq!(value["result"]["passed"], true);
        assert_eq!(value["result"]["summary"]["functions"], 0);
        assert_eq!(value["result"]["summary"]["exceeding"], 0);
        assert_eq!(value["result"]["summary"]["average_crap"], 0.0);
        assert_eq!(value["result"]["summary"]["median_crap"], 0.0);
        assert_eq!(value["result"]["summary"]["risk"]["low"], 0);
        assert!(value["result"]["functions"].as_array().is_some_and(Vec::is_empty));
    }

    #[test]
    fn moderate_does_not_imply_exceeds() {
        let entries = [entry("mid", 20.0, 10, 50.0, Some("demo"), 20)];
        let value = parse(&render_json(&entries, 25.0, Metric::Cyclomatic, false));
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
        let value = parse(&render_json(&entries, 15.0, Metric::Cyclomatic, true));
        assert_eq!(value["result"]["summary"]["median_crap"], 5.0);
        assert_eq!(value["result"]["gate_failed"], true);
    }
}
