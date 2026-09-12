//! Structural integrity scan for TypeScript sources.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::lex::try_skip_noise_token;

/// Checks that comments/strings close and `{}` / `()` / `[]` balance.
pub(super) fn validate_structure(bytes: &[u8]) -> Result<(), &'static str> {
    let mut i = 0;
    let mut stack = Vec::new();
    while i < bytes.len() {
        i = advance_structure(bytes, i, &mut stack)?;
    }
    finish_structure(&stack)
}

fn advance_structure(bytes: &[u8], i: usize, stack: &mut Vec<u8>) -> Result<usize, &'static str> {
    let i = skip_spaces(bytes, i);
    if i >= bytes.len() {
        return Ok(i);
    }
    if let Some(outcome) = try_skip_noise_token(bytes, i) {
        return outcome;
    }
    step_delimiter(bytes, i, stack)
}

const fn finish_structure(stack: &[u8]) -> Result<(), &'static str> {
    if stack.is_empty() {
        Ok(())
    } else {
        Err("unbalanced braces")
    }
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn step_delimiter(bytes: &[u8], i: usize, stack: &mut Vec<u8>) -> Result<usize, &'static str> {
    match bytes[i] {
        open @ (b'{' | b'(' | b'[') => {
            stack.push(open);
            Ok(i + 1)
        }
        close @ (b'}' | b')' | b']') => pop_close(stack, close).map(|()| i + 1),
        _ => Ok(i + 1),
    }
}

fn pop_close(stack: &mut Vec<u8>, close: u8) -> Result<(), &'static str> {
    let Some(open) = stack.pop() else {
        return Err("unbalanced braces");
    };
    if matching_open(close) == open {
        Ok(())
    } else {
        Err("unbalanced braces")
    }
}

const fn matching_open(close: u8) -> u8 {
    match close {
        b'}' => b'{',
        b')' => b'(',
        _ => b'[',
    }
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
