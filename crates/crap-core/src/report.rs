//! Human-readable table and summary for scored functions.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
mod json;

use crate::error::{Error, Result};
use crate::language::{ReportFormat, ScanRequest};
use crate::merge::{CrapEntry, ambiguous_join_warning};
use crate::run::RunResult;
use crate::score::{classify_risk, exceeds_threshold};
use std::env;
use std::io::Write;
use std::path::Path;

#[doc(inline)]
pub use json::{render_json, write_json};

/// Formats the report for `result` using `request.format`.
///
/// # Errors
///
/// Returns [`crate::Error::Report`] if JSON serialization fails.
pub fn render(
    request: &ScanRequest,
    result: &RunResult,
    language: &str,
    color: bool,
) -> Result<String> {
    let mut out = Vec::new();
    write_report(&mut out, request, result, language, color)?;
    utf8_report(out, "report write")
}

fn utf8_report(bytes: Vec<u8>, what: &str) -> Result<String> {
    String::from_utf8(bytes).map_err(|err| Error::report(format!("{what}: {err}")))
}

/// Streams the report for `result` to `w`.
///
/// # Errors
///
/// Returns [`crate::Error::Report`] if writing or JSON serialization fails.
pub fn write_report(
    w: &mut impl Write,
    request: &ScanRequest,
    result: &RunResult,
    language: &str,
    color: bool,
) -> Result<()> {
    let threshold = request.effective_threshold();
    match request.format {
        ReportFormat::Json => write_json(
            &mut *w,
            &result.entries,
            threshold,
            request.metric,
            result.gate_failed,
            language,
        ),
        ReportFormat::Text if request.summary => {
            write_summary(w, &result.entries, threshold, uses_packages(result))
        }
        ReportFormat::Text => write_table(w, &result.entries, threshold, color),
    }
}

fn uses_packages(result: &RunResult) -> bool {
    result.entries.iter().any(|entry| entry.crate_name.is_some())
}

const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

/// Renders the full table plus footer.
#[must_use]
pub fn render_table(entries: &[CrapEntry], threshold: f64, color: bool) -> String {
    let mut out = Vec::new();
    let _ = write_table(&mut out, entries, threshold, color);
    String::from_utf8(out).unwrap_or_default()
}

fn write_table(
    w: &mut impl Write,
    entries: &[CrapEntry],
    threshold: f64,
    color: bool,
) -> Result<()> {
    let widths = Widths::for_entries(entries);
    write_map_report(w, widths.header().as_bytes())?;
    write_table_rows(w, entries, threshold, color, &widths)?;
    write_map_report(w, b"\n")?;
    write_map_report(w, footer(entries, threshold).as_bytes())?;
    write_table_extras(w, entries, threshold)
}

fn write_table_rows(
    w: &mut impl Write,
    entries: &[CrapEntry],
    threshold: f64,
    color: bool,
    widths: &Widths,
) -> Result<()> {
    for entry in entries {
        write_map_report(w, b"\n")?;
        write_map_report(w, widths.row(entry, threshold, color).as_bytes())?;
    }
    Ok(())
}

fn write_table_extras(w: &mut impl Write, entries: &[CrapEntry], threshold: f64) -> Result<()> {
    let ambiguous = ambiguous_join_warning(entries);
    if !ambiguous.is_empty() {
        write_map_report(w, b"\n")?;
        write_map_report(w, ambiguous.as_bytes())?;
    }
    write_table_action(w, entries, threshold)
}

fn write_table_action(w: &mut impl Write, entries: &[CrapEntry], threshold: f64) -> Result<()> {
    if !entries.iter().any(|e| exceeds_threshold(e.crap, threshold)) {
        return Ok(());
    }
    let action = action_line(entries, threshold);
    write_map_report(w, b"\n")?;
    write_map_report(w, action.as_bytes())
}

/// Renders counts and the worst offender, with optional per-crate lines.
#[must_use]
pub fn render_summary(entries: &[CrapEntry], threshold: f64, per_crate: bool) -> String {
    let mut out = Vec::new();
    let _ = write_summary(&mut out, entries, threshold, per_crate);
    String::from_utf8(out).unwrap_or_default()
}

fn write_summary(
    w: &mut impl Write,
    entries: &[CrapEntry],
    threshold: f64,
    per_crate: bool,
) -> Result<()> {
    if per_crate && write_crate_summaries(w, entries, threshold)? {
        write_map_report(w, b"\n")?;
    }
    write_map_report(w, aggregate_line(entries, threshold).as_bytes())
}

fn write_crate_summaries(
    w: &mut impl Write,
    entries: &[CrapEntry],
    threshold: f64,
) -> Result<bool> {
    let mut wrote = false;
    for line in crate_summaries(entries, threshold) {
        if wrote {
            write_map_report(w, b"\n")?;
        }
        write_map_report(w, line.as_bytes())?;
        wrote = true;
    }
    Ok(wrote)
}

fn write_map_report(w: &mut impl Write, bytes: &[u8]) -> Result<()> {
    w.write_all(bytes).map_err(|err| Error::report(format!("report write: {err}")))
}

fn footer(entries: &[CrapEntry], threshold: f64) -> String {
    let over = over_count(entries, threshold);
    format!(
        "{over}/{} functions exceed threshold {threshold}.",
        entries.len()
    )
}

fn action_line(entries: &[CrapEntry], threshold: f64) -> String {
    let failing: Vec<&CrapEntry> =
        entries.iter().filter(|e| exceeds_threshold(e.crap, threshold)).collect();
    let needs_tests = failing.iter().any(|e| e.coverage < 90.0);
    let needs_refactor = failing.iter().any(|e| e.coverage >= 90.0);
    match (needs_tests, needs_refactor) {
        (true, true) => {
            "Add tests where coverage is low; refactor when complexity stays high.".into()
        }
        (true, false) => "Add automated tests for the under-covered functions.".into(),
        (false, true) => "Refactor functions whose complexity stays high even when covered.".into(),
        (false, false) => String::new(),
    }
}

fn aggregate_line(entries: &[CrapEntry], threshold: f64) -> String {
    let over = over_count(entries, threshold);
    let Some(entry) = entries.iter().max_by(|a, b| a.crap.total_cmp(&b.crap)) else {
        return format!("0 functions, 0 exceed threshold {threshold}.");
    };
    format!(
        "{} functions, {over} exceed threshold {threshold}; worst: {} {:.1} ({})",
        entries.len(),
        entry.function,
        entry.crap,
        location(entry),
    )
}

fn crate_summaries(entries: &[CrapEntry], threshold: f64) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().filter_map(|e| e.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names.into_iter().map(|name| crate_line(entries, threshold, &name)).collect()
}

fn crate_line(entries: &[CrapEntry], threshold: f64, name: &str) -> String {
    let subset: Vec<&CrapEntry> =
        entries.iter().filter(|e| e.crate_name.as_deref() == Some(name)).collect();
    let over = over_count(subset.iter().copied(), threshold);
    format!("{name}: {} functions, {over} over", subset.len())
}

fn over_count<'a, I>(entries: I, threshold: f64) -> usize
where
    I: IntoIterator<Item = &'a CrapEntry>,
{
    entries.into_iter().filter(|e| exceeds_threshold(e.crap, threshold)).count()
}

fn location(entry: &CrapEntry) -> String {
    format!("{}:{}", display_path(&entry.file), entry.start_line)
}

pub(crate) fn display_path(path: &Path) -> String {
    env::current_dir().ok().and_then(|cwd| path.strip_prefix(cwd).ok()).map_or_else(
        || path.display().to_string(),
        |rel| rel.display().to_string(),
    )
}

fn paint(text: &str, color: bool) -> String {
    if color {
        format!("{RED}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

struct Widths {
    crap: usize,
    cc: usize,
    cov: usize,
    func: usize,
    risk: usize,
}

impl Widths {
    fn for_entries(entries: &[CrapEntry]) -> Self {
        let mut w = Self {
            crap: 4,
            cc: 2,
            cov: 4,
            func: 8,
            risk: 4,
        };
        for entry in entries {
            w.crap = w.crap.max(format!("{:.1}", entry.crap).len());
            w.cc = w.cc.max(entry.complexity.to_string().len());
            w.cov = w.cov.max(format!("{:.1}", entry.coverage).len());
            w.func = w.func.max(entry.function.len());
            w.risk = w.risk.max(classify_risk(entry.crap).to_string().len());
        }
        w
    }

    fn header(&self) -> String {
        format!(
            "  {:<4}  {:<w_risk$}  {:>w_crap$}  {:>w_cc$}  {:>w_cov$}  {:<w_fn$}  LOCATION",
            "",
            "RISK",
            "CRAP",
            "COMP",
            "COV%",
            "FUNCTION",
            w_risk = self.risk,
            w_crap = self.crap,
            w_cc = self.cc,
            w_cov = self.cov,
            w_fn = self.func,
        )
    }

    fn row(&self, entry: &CrapEntry, threshold: f64, color: bool) -> String {
        let fail = exceeds_threshold(entry.crap, threshold);
        let mark = paint(
            &format!("{:<4}", if fail { "FAIL" } else { "ok" }),
            fail && color,
        );
        format!(
            "  {mark}  {risk:<w_risk$}  {crap:>w_crap$.1}  {cc:>w_cc$}  \
             {cov:>w_cov$.1}  {func:<w_fn$}  {loc}",
            risk = classify_risk(entry.crap),
            crap = entry.crap,
            cc = entry.complexity,
            cov = entry.coverage,
            func = entry.function,
            loc = location(entry),
            w_risk = self.risk,
            w_crap = self.crap,
            w_cc = self.cc,
            w_cov = self.cov,
            w_fn = self.func,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::RunResult;
    use std::path::PathBuf;

    fn entry(name: &str, crap: f64, cc: usize, cov: f64) -> CrapEntry {
        CrapEntry {
            file: PathBuf::from("src/lib.rs"),
            function: name.into(),
            start_line: 1,
            end_line: 1,
            complexity: cc,
            coverage: cov,
            coverage_join: crate::merge::CoverageJoin::Measured,
            crap,
            crate_name: Some("demo".into()),
        }
    }

    fn assert_contains(haystack: &str, needles: &[&str]) {
        for needle in needles {
            assert!(
                haystack.contains(needle),
                "missing {needle:?} in {haystack}"
            );
        }
    }

    #[test]
    fn table_reports_ambiguous_path_joins() {
        let mut entries = [entry("tied", 2.0, 1, 0.0)];
        entries[0].coverage_join = crate::merge::CoverageJoin::Ambiguous;
        let table = render_table(&entries, 30.0, false);
        assert!(table.contains("ambiguous coverage paths"));
    }

    #[test]
    fn table_marks_failures_and_omits_table_in_summary() {
        let entries = [
            entry("crappy", 156.0, 12, 0.0),
            entry("okfn", 1.0, 1, 100.0),
        ];
        let table = render_table(&entries, 30.0, false);
        assert_contains(
            &table,
            &[
                "FAIL",
                "RISK",
                "high",
                "crappy",
                "1/2 functions exceed threshold 30",
            ],
        );
        let summary = render_summary(&entries, 30.0, true);
        assert!(!summary.contains("FUNCTION"));
        assert_contains(&summary, &["demo: 2 functions, 1 over", "worst: crappy"]);
    }

    #[test]
    fn action_line_asks_for_tests_when_coverage_is_low() {
        let entries = [entry("crappy", 156.0, 12, 0.0)];
        let table = render_table(&entries, 30.0, false);
        assert!(table.contains("Add automated tests for the under-covered functions."));
    }

    #[test]
    fn action_line_asks_to_refactor_when_coverage_is_high() {
        let entries = [entry("dense", 40.0, 31, 100.0)];
        let table = render_table(&entries, 30.0, false);
        assert!(
            table.contains("Refactor functions whose complexity stays high even when covered.")
        );
    }

    #[test]
    fn action_line_asks_for_both_when_mixed() {
        let entries = [
            entry("crappy", 156.0, 12, 0.0),
            entry("dense", 40.0, 31, 100.0),
        ];
        let table = render_table(&entries, 30.0, false);
        assert!(
            table.contains("Add tests where coverage is low; refactor when complexity stays high.")
        );
    }

    #[test]
    fn action_line_is_omitted_when_nothing_fails() {
        let entries = [entry("okfn", 1.0, 1, 100.0)];
        let table = render_table(&entries, 30.0, false);
        assert!(!table.contains("Add automated tests"));
        assert!(!table.contains("Refactor functions"));
        assert_eq!(action_line(&entries, 30.0), "");
        assert_eq!(action_line(&[], 30.0), "");
    }

    #[test]
    fn empty_summary_has_zero_functions() {
        let summary = render_summary(&[], 30.0, false);
        assert_eq!(summary, "0 functions, 0 exceed threshold 30.");
    }

    #[test]
    fn moderate_row_can_pass_a_lenient_gate() {
        let entries = [entry("mid", 20.0, 10, 50.0)];
        let table = render_table(&entries, 25.0, false);
        assert!(table.contains("ok"));
        assert!(table.contains("moderate"));
        assert!(!table.contains("FAIL"));
    }

    #[test]
    fn fail_rows_use_ansi_when_color_is_on() {
        let entries = [entry("crappy", 156.0, 12, 0.0)];
        let table = render_table(&entries, 30.0, true);
        assert!(table.contains("\u{1b}[31m"));
        assert!(table.contains("\u{1b}[0m"));
    }

    #[test]
    fn write_report_maps_io_errors() {
        struct Fail;
        impl Write for Fail {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let request = ScanRequest {
            path: PathBuf::from("."),
            coverage: PathBuf::from("lcov.info"),
            metric: crate::Metric::Cyclomatic,
            threshold: None,
            summary: false,
            fail_above: false,
            missing: crate::MissingPolicy::Pessimistic,
            format: ReportFormat::Text,
        };
        let result = RunResult::default();
        let mut fail = Fail;
        let err = write_report(&mut fail, &request, &result, "rust", false);
        assert!(err.is_err(), "{err:?}");
        assert!(fail.flush().is_ok());
    }

    #[test]
    fn utf8_report_maps_invalid_bytes() {
        let err = utf8_report(vec![0xff], "report write");
        assert!(err.is_err(), "{err:?}");
        assert!(utf8_report(b"ok".to_vec(), "report write").is_ok());
    }
}
