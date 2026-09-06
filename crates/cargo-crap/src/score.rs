//! Change-risk score from complexity and coverage.

/// Usual gate: scores strictly above this are treated as risky.
pub const DEFAULT_THRESHOLD: f64 = 30.0;

/// Combines complexity and coverage percent into one score.
///
/// The formula is `comp² × (1 − cov/100)³ + comp`. Coverage outside
/// `[0, 100]` is clamped. At 100% coverage the score equals complexity.
///
/// # Examples
///
/// ```
/// use cargo_crap::score::crap;
/// assert_eq!(crap(1.0, 100.0), 1.0);
/// assert_eq!(crap(6.0, 0.0), 42.0);
/// ```
#[must_use]
pub fn crap(complexity: f64, coverage_pct: f64) -> f64 {
    let uncovered = 1.0 - (coverage_pct.clamp(0.0, 100.0) / 100.0);
    complexity.powi(2).mul_add(uncovered.powi(3), complexity)
}

/// Returns whether `score` is strictly above `threshold`.
#[must_use]
pub fn exceeds_threshold(score: f64, threshold: f64) -> bool {
    score > threshold
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "the formula is deterministic; exact equality is the contract"
)]
mod tests {
    use super::*;

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
        assert!(crap(31.0, 100.0) > DEFAULT_THRESHOLD);
        assert!(exceeds_threshold(crap(31.0, 100.0), DEFAULT_THRESHOLD));
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
}
