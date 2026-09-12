//! Shared line-boundary helpers for Python source scans.

/// Tab display width used when measuring leading indentation (PEP 8).
pub(super) const TAB_WIDTH: usize = 8;

/// Returns the exclusive end index of the line starting at `start` (past `\n` if present).
pub(super) fn line_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    if i < bytes.len() { i + 1 } else { i }
}

/// Returns `(indent_cols, content_index)` for `[start, end)`.
///
/// Tabs expand to the next multiple of [`TAB_WIDTH`]. Mixing tabs and spaces in
/// the leading whitespace is an error.
pub(super) fn leading_indent(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Result<(usize, usize), &'static str> {
    let mut i = start;
    let mut indent = 0_usize;
    let mut seen_space = false;
    let mut seen_tab = false;
    while i < end && i < bytes.len() {
        match bytes[i] {
            b' ' => {
                if seen_tab {
                    return Err("mixed tabs and spaces in indentation");
                }
                seen_space = true;
                indent += 1;
                i += 1;
            }
            b'\t' => {
                if seen_space {
                    return Err("mixed tabs and spaces in indentation");
                }
                seen_tab = true;
                indent = next_tab_stop(indent);
                i += 1;
            }
            _ => break,
        }
    }
    Ok((indent, i))
}

const fn next_tab_stop(indent: usize) -> usize {
    (indent / TAB_WIDTH + 1) * TAB_WIDTH
}

/// True when `content` is past EOF or points at a blank/comment line start.
pub(super) fn is_blank_or_comment_at(bytes: &[u8], content: usize) -> bool {
    content >= bytes.len() || matches!(bytes[content], b'#' | b'\n' | b'\r')
}

/// Rejects mixed tabs/spaces in leading whitespace on any physical line.
pub(super) fn validate_indents(bytes: &[u8]) -> Result<(), &'static str> {
    let mut start = 0;
    while start <= bytes.len() {
        let end = line_end(bytes, start);
        leading_indent(bytes, start, end)?;
        if end >= bytes.len() {
            break;
        }
        start = end;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
