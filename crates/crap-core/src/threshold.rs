//! Named and numeric pass/fail threshold parsing for CLIs.

/// `--threshold strict` gate value.
pub const STRICT: f64 = 8.0;

/// `--threshold lenient` gate value.
pub const LENIENT: f64 = 25.0;

/// Parses a `--threshold` value: `strict`, `lenient`, or a non-negative number.
///
/// # Errors
///
/// Returns a usage message when the text is not a known preset or a finite
/// non-negative number.
pub fn parse_threshold(text: &str) -> std::result::Result<f64, String> {
    if let Some(preset) = named_threshold(text) {
        return Ok(preset);
    }
    parse_numeric_threshold(text)
}

fn named_threshold(text: &str) -> Option<f64> {
    match text {
        "strict" => Some(STRICT),
        "lenient" => Some(LENIENT),
        _ => None,
    }
}

fn parse_numeric_threshold(text: &str) -> std::result::Result<f64, String> {
    let value: f64 = text.parse().map_err(|_| format!("invalid --threshold `{text}`"))?;
    if !value.is_finite() || value < 0.0 {
        return Err("--threshold must be a non-negative number or strict|lenient".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::assert_f64_bits_eq;

    #[test]
    fn presets_and_numbers() {
        assert_f64_bits_eq(parse_threshold("strict").unwrap_or(-1.0), STRICT);
        assert_f64_bits_eq(parse_threshold("lenient").unwrap_or(-1.0), LENIENT);
        assert_f64_bits_eq(parse_threshold("15").unwrap_or(-1.0), 15.0);
        assert!(parse_threshold("nope").is_err());
        assert!(parse_threshold("-1").is_err());
    }
}
