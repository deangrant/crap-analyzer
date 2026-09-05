//! Human-readable table and summary for scored functions.

use crate::merge::CrapEntry;
use crate::score::exceeds_threshold;
use std::env;
use std::io::IsTerminal;
use std::path::Path;

const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

/// Renders the full table plus footer.
#[must_use]
pub fn render_table(entries: &[CrapEntry], threshold: f64, color: bool) -> String {
    let widths = Widths::for_entries(entries);
    let mut lines = vec![widths.header()];
    for entry in entries {
        lines.push(widths.row(entry, threshold, color));
    }
    lines.push(footer(entries, threshold));
    if entries.iter().any(|e| exceeds_threshold(e.crap, threshold)) {
        lines.push(action_line(entries, threshold));
    }
    lines.join("\n")
}

/// Renders counts and the worst offender, with optional per-crate lines.
#[must_use]
pub fn render_summary(entries: &[CrapEntry], threshold: f64, per_crate: bool) -> String {
    let mut lines = Vec::new();
    if per_crate {
        lines.extend(crate_summaries(entries, threshold));
    }
    lines.push(aggregate_line(entries, threshold));
    lines.join("\n")
}

/// True when stdout is a TTY and `NO_COLOR` is unset.
#[must_use]
pub fn color_enabled() -> bool {
    env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
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
    format!("{}:{}", display_path(&entry.file), entry.line)
}

fn display_path(path: &Path) -> String {
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
}

impl Widths {
    fn for_entries(entries: &[CrapEntry]) -> Self {
        let mut w = Self {
            crap: 4,
            cc: 2,
            cov: 4,
            func: 8,
        };
        for entry in entries {
            w.crap = w.crap.max(format!("{:.1}", entry.crap).len());
            w.cc = w.cc.max(entry.cyclomatic.to_string().len());
            w.cov = w.cov.max(format!("{:.1}", entry.coverage).len());
            w.func = w.func.max(entry.function.len());
        }
        w
    }

    fn header(&self) -> String {
        format!(
            "  {:<4}  {:>w_crap$}  {:>w_cc$}  {:>w_cov$}  {:<w_fn$}  LOCATION",
            "",
            "CRAP",
            "CC",
            "COV%",
            "FUNCTION",
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
            "  {mark}  {crap:>w_crap$.1}  {cc:>w_cc$}  {cov:>w_cov$.1}  {func:<w_fn$}  {loc}",
            crap = entry.crap,
            cc = entry.cyclomatic,
            cov = entry.coverage,
            func = entry.function,
            loc = location(entry),
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
    use std::path::PathBuf;

    fn entry(name: &str, crap: f64, cc: usize, cov: f64) -> CrapEntry {
        CrapEntry {
            file: PathBuf::from("src/lib.rs"),
            function: name.into(),
            line: 1,
            cyclomatic: cc,
            coverage: cov,
            crap,
            crate_name: Some("demo".into()),
        }
    }

    #[test]
    fn table_marks_failures_and_omits_table_in_summary() {
        let entries = [
            entry("crappy", 156.0, 12, 0.0),
            entry("okfn", 1.0, 1, 100.0),
        ];
        let table = render_table(&entries, 30.0, false);
        assert!(table.contains("FAIL"));
        assert!(table.contains("crappy"));
        assert!(table.contains("1/2 functions exceed threshold 30"));
        let summary = render_summary(&entries, 30.0, true);
        assert!(!summary.contains("FUNCTION"));
        assert!(summary.contains("demo: 2 functions, 1 over"));
        assert!(summary.contains("worst: crappy"));
    }
}
