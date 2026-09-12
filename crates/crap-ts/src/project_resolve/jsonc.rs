//! Strip JSONC comments and trailing commas before `serde_json` parse.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.

/// Remove `//` / `/* */` comments and trailing commas outside JSON strings.
#[must_use]
pub(super) fn strip_jsonc(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        i = push_jsonc_byte(bytes, i, &mut out);
    }
    out
}

fn push_jsonc_byte(bytes: &[u8], i: usize, out: &mut String) -> usize {
    match bytes[i] {
        b'"' => push_string(bytes, i, out),
        b'/' if bytes.get(i + 1) == Some(&b'/') => skip_line_comment(bytes, i),
        b'/' if bytes.get(i + 1) == Some(&b'*') => skip_block_comment(bytes, i),
        b',' => push_comma(bytes, i, out),
        b => {
            out.push(char::from(b));
            i + 1
        }
    }
}

fn push_string(bytes: &[u8], start: usize, out: &mut String) -> usize {
    out.push('"');
    let mut i = start + 1;
    while i < bytes.len() {
        let b = bytes[i];
        out.push(char::from(b));
        if b == b'\\'
            && let Some(&next) = bytes.get(i + 1)
        {
            out.push(char::from(next));
            i += 2;
            continue;
        }
        if b == b'"' {
            return i + 1;
        }
        i += 1;
    }
    i
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    bytes.len()
}

fn push_comma(bytes: &[u8], i: usize, out: &mut String) -> usize {
    let next = skip_jsonc_space(bytes, i + 1);
    if matches!(bytes.get(next), Some(&b'}' | &b']')) {
        return next;
    }
    out.push(',');
    i + 1
}

fn skip_jsonc_space(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if is_jsonc_space(bytes[i]) {
            i += 1;
            continue;
        }
        match try_skip_comment(bytes, i) {
            Some(next) => i = next,
            None => break,
        }
    }
    i
}

const fn is_jsonc_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn try_skip_comment(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'/') {
        return None;
    }
    match bytes.get(i + 1) {
        Some(&b'/') => Some(skip_line_comment(bytes, i)),
        Some(&b'*') => Some(skip_block_comment(bytes, i)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::strip_jsonc;

    #[test]
    fn strips_line_and_block_comments() {
        let got = strip_jsonc("{\n  // note\n  \"name\": \"demo\" /* x */\n}\n");
        assert!(got.contains("\"name\""));
        assert!(!got.contains("//"));
        assert!(!got.contains("/*"));
    }

    #[test]
    fn strips_trailing_commas() {
        assert_eq!(strip_jsonc(r#"{"a":1,}"#), r#"{"a":1}"#);
        assert_eq!(strip_jsonc("[1,2,]"), "[1,2]");
    }

    #[test]
    fn strips_trailing_comma_before_comment() {
        assert_eq!(strip_jsonc("{\"a\":1, // x\n}"), "{\"a\":1}");
        assert_eq!(strip_jsonc("{\"a\":1, /* x */ }"), "{\"a\":1}");
    }

    #[test]
    fn keeps_slashes_inside_strings() {
        assert_eq!(
            strip_jsonc(r#"{"url":"http://x"}"#),
            r#"{"url":"http://x"}"#
        );
    }

    #[test]
    fn unclosed_string_and_block_comment_reach_eof() {
        assert!(strip_jsonc("\"oops").ends_with('s'));
        assert_eq!(strip_jsonc("/* oops"), "");
    }

    #[test]
    fn lone_slash_is_not_a_comment() {
        assert_eq!(strip_jsonc(r#"{"a":1,/}"#), r#"{"a":1,/}"#);
    }
}
