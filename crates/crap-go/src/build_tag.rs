//! Evaluate Go build constraints at the top of a source file.

/// Returns true when `source` should be skipped for the enabled `tags`.
#[must_use]
pub fn skip_file(source: &str, tags: &[String]) -> bool {
    let Some(expr) = leading_constraint(source) else {
        return false;
    };
    !eval_expr(&expr, tags)
}

fn leading_constraint(source: &str) -> Option<String> {
    let mut go_build = None;
    let mut plus_build = Vec::new();
    for line in source.lines() {
        if !take_constraint_line(line.trim(), &mut go_build, &mut plus_build) {
            break;
        }
    }
    constraint_from_parts(go_build, &plus_build)
}

fn take_constraint_line(
    trimmed: &str,
    go_build: &mut Option<String>,
    plus_build: &mut Vec<String>,
) -> bool {
    if trimmed.is_empty() {
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("//go:build") {
        *go_build = Some(rest.trim().to_owned());
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("// +build") {
        plus_build.push(rest.trim().to_owned());
        return true;
    }
    trimmed.starts_with("//")
}

fn constraint_from_parts(go_build: Option<String>, plus_build: &[String]) -> Option<String> {
    go_build.or_else(|| (!plus_build.is_empty()).then(|| plus_lines_to_expr(plus_build)))
}

fn plus_lines_to_expr(lines: &[String]) -> String {
    let and_groups: Vec<String> = lines
        .iter()
        .map(|line| {
            let alts: Vec<String> = line
                .split_whitespace()
                .map(|tok| {
                    tok.strip_prefix('!').map_or_else(|| tok.to_owned(), |rest| format!("!{rest}"))
                })
                .collect();
            if alts.len() == 1 {
                alts[0].clone()
            } else {
                format!("({})", alts.join(" || "))
            }
        })
        .collect();
    and_groups.join(" && ")
}

fn eval_expr(expr: &str, tags: &[String]) -> bool {
    let tokens = tokenize(expr);
    let mut parser = Parser {
        tokens: &tokens,
        index: 0,
        tags,
    };
    parser.parse_or().unwrap_or(false)
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    tags: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Not,
    And,
    Or,
    LParen,
    RParen,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn bump(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.index)?;
        self.index += 1;
        Some(tok)
    }

    fn parse_or(&mut self) -> Option<bool> {
        let mut value = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.bump();
            value |= self.parse_and()?;
        }
        Some(value)
    }

    fn parse_and(&mut self) -> Option<bool> {
        let mut value = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.bump();
            value &= self.parse_unary()?;
        }
        Some(value)
    }

    fn parse_unary(&mut self) -> Option<bool> {
        if matches!(self.peek(), Some(Token::Not)) {
            self.bump();
            return Some(!self.parse_unary()?);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<bool> {
        match self.bump()?.clone() {
            Token::Ident(name) => Some(tag_enabled(&name, self.tags)),
            Token::LParen => self.parse_paren(),
            Token::Not | Token::And | Token::Or | Token::RParen => None,
        }
    }

    fn parse_paren(&mut self) -> Option<bool> {
        let value = self.parse_or()?;
        if !matches!(self.bump(), Some(Token::RParen)) {
            return None;
        }
        Some(value)
    }
}

fn tag_enabled(name: &str, tags: &[String]) -> bool {
    if tags.iter().any(|t| t == name) {
        return true;
    }
    host_tag(name)
}

fn host_tag(name: &str) -> bool {
    if let Some(enabled) = known_os_tag(name) {
        return enabled;
    }
    name == "unix" && is_unix_host()
}

fn known_os_tag(name: &str) -> Option<bool> {
    if let Some(enabled) = primary_goos(name) {
        return Some(enabled);
    }
    if let Some(enabled) = secondary_goos(name) {
        return Some(enabled);
    }
    // No stable rustc host mapping; enable only via `--tags`.
    matches!(name, "plan9" | "js" | "zos" | "hurd").then_some(false)
}

fn primary_goos(name: &str) -> Option<bool> {
    match name {
        "linux" => Some(cfg!(target_os = "linux")),
        "windows" => Some(cfg!(target_os = "windows")),
        "darwin" => Some(cfg!(target_os = "macos")),
        "freebsd" => Some(cfg!(target_os = "freebsd")),
        "netbsd" => Some(cfg!(target_os = "netbsd")),
        "openbsd" => Some(cfg!(target_os = "openbsd")),
        _ => None,
    }
}

fn secondary_goos(name: &str) -> Option<bool> {
    match name {
        "dragonfly" => Some(cfg!(target_os = "dragonfly")),
        "android" => Some(cfg!(target_os = "android")),
        "ios" => Some(cfg!(target_os = "ios")),
        "illumos" => Some(cfg!(target_os = "illumos")),
        "solaris" => Some(cfg!(target_os = "solaris")),
        "aix" => Some(cfg!(target_os = "aix")),
        _ => None,
    }
}

const fn is_unix_host() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
        target_os = "android",
        target_os = "ios",
    ))
}

fn tokenize(expr: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if matches!(ch, ' ' | '\t') {
            chars.next();
            continue;
        }
        if push_token(&mut tokens, &mut chars, ch) {
            continue;
        }
        chars.next();
    }
    tokens
}

fn push_token(
    tokens: &mut Vec<Token>,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ch: char,
) -> bool {
    if push_punct(tokens, chars, ch) {
        return true;
    }
    if is_ident_start(ch) {
        tokens.push(Token::Ident(read_ident_chars(chars)));
        return true;
    }
    false
}

fn push_punct(
    tokens: &mut Vec<Token>,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ch: char,
) -> bool {
    if let Some(token) = single_punct(ch) {
        chars.next();
        tokens.push(token);
        return true;
    }
    push_double_punct(tokens, chars, ch)
}

const fn single_punct(ch: char) -> Option<Token> {
    match ch {
        '!' => Some(Token::Not),
        '(' => Some(Token::LParen),
        ')' => Some(Token::RParen),
        _ => None,
    }
}

fn push_double_punct(
    tokens: &mut Vec<Token>,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ch: char,
) -> bool {
    match ch {
        '&' => {
            push_double(chars, tokens, '&', Token::And);
            true
        }
        '|' => {
            push_double(chars, tokens, '|', Token::Or);
            true
        }
        _ => false,
    }
}

fn push_double(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    tokens: &mut Vec<Token>,
    expected: char,
    token: Token,
) {
    chars.next();
    if chars.next() == Some(expected) {
        tokens.push(token);
    }
}

fn read_ident_chars(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if is_ident_continue(c) {
            ident.push(c);
            chars.next();
        } else {
            break;
        }
    }
    ident
}

const fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

const fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

#[cfg(test)]
#[path = "build_tag_tests.rs"]
mod tests;
