//! Versioned JSON envelope for automation.

use super::display_path;
use crate::error::{Error, Result};
use crate::merge::{CrapEntry, ambiguous_join_count};
use crate::metric::Metric;
use crate::score::{Risk, classify_risk, exceeds_threshold, to_f64};
use serde::Serialize;
use std::io::Write;

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
    let mut out = Vec::new();
    let written = write_json(&mut out, entries, threshold, metric, gate_failed, language);
    finish_json_buffer(written, out)
}

fn finish_json_buffer(written: Result<()>, bytes: Vec<u8>) -> Result<String> {
    written?;
    utf8_json(bytes)
}

fn utf8_json(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|err| Error::report(format!("json report: {err}")))
}

/// Streams a schema-versioned JSON document to `w`, one function at a time.
///
/// # Errors
///
/// Returns [`Error::Report`] if writing or serialization fails.
pub fn write_json(
    w: &mut impl Write,
    entries: &[CrapEntry],
    threshold: f64,
    metric: Metric,
    gate_failed: bool,
    language: &str,
) -> Result<()> {
    let exceeding = entries.iter().filter(|e| exceeds_threshold(e.crap, threshold)).count();
    let scores: Vec<f64> = entries.iter().map(|e| e.crap).collect();
    let summary = SummaryDoc {
        functions: entries.len(),
        exceeding,
        ambiguous: ambiguous_join_count(entries),
        average_crap: average(&scores),
        median_crap: median(&scores),
        risk: risk_counts(entries),
    };
    write_json_envelope(
        w,
        language,
        metric,
        threshold,
        exceeding == 0,
        gate_failed,
        &summary,
    )?;
    match write_json_functions(w, entries, threshold) {
        Ok(()) => write_map_err(w, b"\n  }\n}\n"),
        Err(err) => Err(err),
    }
}

fn write_json_envelope(
    w: &mut impl Write,
    language: &str,
    metric: Metric,
    threshold: f64,
    threshold_cleared: bool,
    gate_failed: bool,
    summary: &SummaryDoc,
) -> Result<()> {
    let mut header = String::new();
    header.push_str("{\n  \"schema_version\": 4,\n  \"language\": ");
    header.push_str(&json_str(language));
    header.push_str(",\n  \"metric\": ");
    header.push_str(&json_str(&metric.to_string()));
    header.push_str(",\n  \"threshold\": ");
    header.push_str(&json_number(threshold));
    header.push_str(",\n  \"result\": {\n    \"threshold_cleared\": ");
    header.push_str(if threshold_cleared { "true" } else { "false" });
    header.push_str(",\n    \"gate_failed\": ");
    header.push_str(if gate_failed { "true" } else { "false" });
    header.push_str(",\n    \"summary\": ");
    write_map_err(w, header.as_bytes())?;
    write_pretty_value(w, summary, 4)?;
    write_map_err(w, b",\n    \"functions\": [")
}

fn write_json_functions(w: &mut impl Write, entries: &[CrapEntry], threshold: f64) -> Result<()> {
    for (index, entry) in entries.iter().enumerate() {
        write_one_function(w, index, entry, threshold)?;
    }
    write_map_err(w, functions_closer(entries.is_empty()))
}

fn write_one_function(
    w: &mut impl Write,
    index: usize,
    entry: &CrapEntry,
    threshold: f64,
) -> Result<()> {
    let sep: &[u8] = if index == 0 { b"\n" } else { b",\n" };
    write_map_err(w, sep)?;
    write_pretty_value(w, &function_doc(entry, threshold), 6)
}

const fn functions_closer(empty: bool) -> &'static [u8] {
    if empty { b"]" } else { b"\n    ]" }
}

fn write_pretty_value(w: &mut impl Write, value: &impl Serialize, indent: usize) -> Result<()> {
    let text = pretty_text(value);
    let pad = " ".repeat(indent);
    let mut block = String::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            block.push('\n');
        }
        block.push_str(&pad);
        block.push_str(line);
    }
    write_map_err(w, block.as_bytes())
}

fn pretty_text(value: &impl Serialize) -> String {
    pretty_text_from(serde_json::to_string_pretty(value))
}

fn pretty_text_from(result: std::result::Result<String, serde_json::Error>) -> String {
    result.unwrap_or_else(|_| String::new())
}

fn json_str(value: &str) -> String {
    serde_string_from(serde_json::to_string(value), "\"\"")
}

fn json_number(value: f64) -> String {
    serde_string_from(serde_json::to_string(&value), "0.0")
}

fn serde_string_from(
    result: std::result::Result<String, serde_json::Error>,
    fallback: &str,
) -> String {
    result.unwrap_or_else(|_| fallback.to_owned())
}

fn write_map_err(w: &mut impl Write, bytes: &[u8]) -> Result<()> {
    w.write_all(bytes).map_err(|err| Error::report(format!("json report: {err}")))
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
        return 0.0;
    }
    scores.iter().sum::<f64>() / to_f64(scores.len())
}

fn median(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let mut ordered = scores.to_vec();
    ordered.sort_by(f64::total_cmp);
    let mid = ordered.len() / 2;
    if ordered.len() % 2 == 1 {
        return ordered[mid];
    }
    f64::midpoint(ordered[mid - 1], ordered[mid])
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
