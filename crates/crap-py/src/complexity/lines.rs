//! Shared line-boundary helpers for Python source scans.

/// Returns the exclusive end index of the line starting at `start` (past `\n` if present).
pub(super) fn line_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    if i < bytes.len() { i + 1 } else { i }
}

/// Returns `(indent_cols, content_index)` for `[start, end)`.
pub(super) fn leading_indent(bytes: &[u8], start: usize, end: usize) -> (usize, usize) {
    let mut i = start;
    let mut indent = 0_usize;
    while i < end && i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' => {
                indent += 1;
                i += 1;
            }
            _ => break,
        }
    }
    (indent, i)
}

/// True when `content` is past EOF or points at a blank/comment line start.
pub(super) fn is_blank_or_comment_at(bytes: &[u8], content: usize) -> bool {
    content >= bytes.len() || matches!(bytes[content], b'#' | b'\n' | b'\r')
}
