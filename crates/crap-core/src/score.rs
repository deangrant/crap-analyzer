//! Change-risk score from complexity and coverage.

use std::fmt;
use std::str::FromStr;

/// Combines complexity and coverage percent into one score.
///
/// The formula is `comp² × (1 − cov/100)³ + comp`. Coverage outside
/// `[0, 100]` is clamped. At 100% coverage the score equals complexity.
///
/// # Examples
///
/// ```
/// use crap_core::score::crap;
/// assert_eq!(crap(1.0, 100.0), 1.0);
/// assert_eq!(crap(6.0, 0.0), 42.0);
/// ```
#[must_use]
pub fn crap(complexity: f64, coverage_pct: f64) -> f64 {
    let uncovered = 1.0 - (coverage_pct.clamp(0.0, 100.0) / 100.0);
    complexity.powi(2).mul_add(uncovered.powi(3), complexity)
}

/// Returns whether `score` is strictly above `threshold`.
///
/// This is the pass/fail gate. It is independent of [`classify_risk`].
#[must_use]
pub fn exceeds_threshold(score: f64, threshold: f64) -> bool {
    score > threshold
}

/// Fixed score band. Independent of the threshold gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// Score at or below 8.
    Low,
    /// Score above 8 and at or below 15.
    Acceptable,
    /// Score above 15 and at or below 25.
    Moderate,
    /// Score above 25, or a non-finite score.
    High,
}

/// Classifies `score` into a fixed band.
///
/// Cutoffs never change with `--threshold`. Non-finite scores are High.
#[must_use]
pub fn classify_risk(score: f64) -> Risk {
    if !score.is_finite() || score > 25.0 {
        Risk::High
    } else if score > 15.0 {
        Risk::Moderate
    } else if score > 8.0 {
        Risk::Acceptable
    } else {
        Risk::Low
    }
}

const RISK_NAMES: &[(&str, Risk)] = &[
    ("low", Risk::Low),
    ("acceptable", Risk::Acceptable),
    ("moderate", Risk::Moderate),
    ("high", Risk::High),
];

impl FromStr for Risk {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        RISK_NAMES
            .iter()
            .find(|(name, _)| *name == value)
            .map(|(_, risk)| *risk)
            .ok_or_else(|| format!("invalid risk `{value}`"))
    }
}

impl fmt::Display for Risk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Acceptable => "acceptable",
            Self::Moderate => "moderate",
            Self::High => "high",
        })
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "the formula is deterministic; exact equality is the contract"
)]
mod tests {
    use super::*;
    use crate::metric::Metric;

    #[test]
    fn trivial_fully_covered_scores_one() {
        assert_eq!(crap(1.0, 100.0), 1.0);
    }

    #[test]
    fn untested_complexity_six_scores_forty_two() {
        assert_eq!(crap(6.0, 0.0), 42.0);
    }

    #[test]
    fn half_covered_complexity_fifteen_scores_forty_three_and_an_eighth() {
        assert_eq!(crap(15.0, 50.0), 43.125);
    }

    #[test]
    fn full_coverage_equals_complexity() {
        assert_eq!(crap(20.0, 100.0), 20.0);
        assert_eq!(crap(5.0, 100.0), 5.0);
        assert_ne!(crap(8.0, 100.0), 0.0);
    }

    #[test]
    fn zero_coverage_is_square_plus_linear() {
        assert_eq!(crap(4.0, 0.0), 20.0);
        assert_eq!(crap(10.0, 0.0), 110.0);
    }

    #[test]
    fn complexity_thirty_one_stays_over_default_when_fully_covered() {
        let threshold = Metric::Cyclomatic.default_threshold();
        assert!(crap(31.0, 100.0) > threshold);
        assert!(exceeds_threshold(crap(31.0, 100.0), threshold));
    }

    #[test]
    fn score_rises_with_complexity_at_fixed_coverage() {
        for cov in [0.0, 25.0, 50.0, 75.0, 100.0] {
            assert!(crap(2.0, cov) <= crap(5.0, cov));
            assert!(crap(5.0, cov) <= crap(10.0, cov));
        }
    }

    #[test]
    fn score_falls_or_holds_as_coverage_rises() {
        for cc in [1.0, 3.0, 10.0, 25.0] {
            let mut prev = f64::INFINITY;
            for cov in [0.0, 25.0, 50.0, 75.0, 100.0] {
                let score = crap(cc, cov);
                assert!(score <= prev);
                prev = score;
            }
        }
    }

    #[test]
    fn coverage_is_clamped() {
        assert_eq!(crap(5.0, -10.0), crap(5.0, 0.0));
        assert_eq!(crap(5.0, 150.0), crap(5.0, 100.0));
    }

    #[test]
    fn threshold_is_strict() {
        assert!(!exceeds_threshold(30.0, 30.0));
        assert!(exceeds_threshold(30.0001, 30.0));
    }

    #[test]
    fn classify_risk_edges() {
        assert_eq!(classify_risk(0.0), Risk::Low);
        assert_eq!(classify_risk(8.0), Risk::Low);
        assert_eq!(classify_risk(8.001), Risk::Acceptable);
        assert_eq!(classify_risk(15.0), Risk::Acceptable);
        assert_eq!(classify_risk(15.001), Risk::Moderate);
        assert_eq!(classify_risk(25.0), Risk::Moderate);
        assert_eq!(classify_risk(25.001), Risk::High);
    }

    #[test]
    fn non_finite_score_is_high() {
        assert_eq!(classify_risk(f64::NAN), Risk::High);
        assert_eq!(classify_risk(f64::INFINITY), Risk::High);
    }

    #[test]
    fn moderate_can_pass_a_lenient_gate() {
        assert_eq!(classify_risk(20.0), Risk::Moderate);
        assert!(!exceeds_threshold(20.0, 25.0));
    }

    #[test]
    fn low_can_fail_a_tight_gate() {
        assert_eq!(classify_risk(6.0), Risk::Low);
        assert!(exceeds_threshold(6.0, 5.0));
    }

    #[test]
    fn parses_and_displays_risk_names() {
        assert_eq!("low".parse::<Risk>().ok(), Some(Risk::Low));
        assert_eq!("acceptable".parse::<Risk>().ok(), Some(Risk::Acceptable));
        assert_eq!("moderate".parse::<Risk>().ok(), Some(Risk::Moderate));
        assert_eq!("high".parse::<Risk>().ok(), Some(Risk::High));
        assert!("nope".parse::<Risk>().is_err());
        assert_eq!(Risk::Low.to_string(), "low");
        assert_eq!(Risk::Acceptable.to_string(), "acceptable");
        assert_eq!(Risk::Moderate.to_string(), "moderate");
        assert_eq!(Risk::High.to_string(), "high");
    }

    fn min_coverage_pct(cc: f64, threshold: f64) -> Option<f64> {
        if cc > threshold {
            return None;
        }
        if crap(cc, 0.0) <= threshold {
            return Some(0.0);
        }
        let uncovered = ((threshold - cc) / cc.powi(2)).cbrt();
        Some((1.0 - uncovered) * 100.0)
    }

    fn near(got: Option<f64>, want: f64) {
        assert!(
            got.is_some_and(|value| (value - want).abs() < 0.5),
            "got {got:?}, want ~{want}"
        );
    }

    #[test]
    fn readme_band_endpoints_match_the_formula() {
        let gate = 30.0;
        near(min_coverage_pct(5.0, gate), 0.0);
        near(min_coverage_pct(6.0, gate), 13.0);
        near(min_coverage_pct(10.0, gate), 42.0);
        near(min_coverage_pct(11.0, gate), 46.0);
        near(min_coverage_pct(15.0, gate), 59.0);
        near(min_coverage_pct(16.0, gate), 62.0);
        near(min_coverage_pct(20.0, gate), 71.0);
        near(min_coverage_pct(21.0, gate), 73.0);
        near(min_coverage_pct(25.0, gate), 80.0);
        near(min_coverage_pct(26.0, gate), 82.0);
        near(min_coverage_pct(30.0, gate), 100.0);
        assert!(min_coverage_pct(31.0, gate).is_none());
    }
}
