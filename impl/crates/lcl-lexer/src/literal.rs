//! Closed literal profiles for constructor `STRING` arguments.
//!
//! Each validator implements one closed contract of the canonical package
//! over the **decoded** string value, exactly as the profiles require ("The
//! constructor first decodes the LCL STRING exactly once. This profile then
//! consumes that complete decoded text"). They are total, allocation-light,
//! and evaluate nothing: no matching, no resolution, no I/O.
//!
//! | Validator | Contract |
//! | --- | --- |
//! | [`regex_flags`] | `types_v0.1.0.json#/pattern_profiles/REGEX` flag fields |
//! | [`regex_pattern`] | `pattern_profiles/REGEX` `grammar`, `escape`, `character_class`, `quantifiers` |
//! | [`glob_pattern`] | `pattern_profiles/GLOB` `segments`, `literal`, `escape`, `character_class` |
//! | [`date`], [`time`], [`datetime`] | `types_v0.1.0.json#/temporal_literal_contract` |
//! | [`uri`] | `operators_and_functions_v0.1.0.json#/constructors/URI`: RFC 3986 absolute-URI with a scheme |
//!
//! Every profile names `error.literal.invalid` as its error, which the
//! registry stages as lexical; [`crate::Lexicon::load`] checks that before any
//! of these runs. `PATH` is deliberately absent: its `error.literal.invalid` has
//! no closed literal profile in the package, and its absolute/relative legality
//! depends on the receiving field and on resolution.

use crate::lexicon::RegexFlagContract;
use std::cmp::Ordering;

// ---------------------------------------------------------------------------
// REGEX flags
// ---------------------------------------------------------------------------

/// "The optional flags STRING contains i, m, and s at most once, in canonical
/// ims order; omission means the empty STRING." The letters, the order, and the
/// duplicate/unknown policy come from the contract, not from this file.
pub(crate) fn regex_flags(flags: &str, contract: &RegexFlagContract) -> Result<(), String> {
    let mut last_rank: Option<usize> = None;
    let mut seen: Vec<char> = Vec::new();
    for c in flags.chars() {
        let rank = contract.canonical_order.iter().position(|&o| o == c);
        let Some(rank) = rank.filter(|_| contract.allowed.contains(&c)) else {
            if contract.unknown_allowed {
                continue;
            }
            return Err(format!(
                "flag `{c}` is not one of {}",
                render_chars(&contract.allowed)
            ));
        };
        if seen.contains(&c) && !contract.duplicates_allowed {
            return Err(format!("flag `{c}` occurs more than once"));
        }
        if let Some(previous) = last_rank {
            if rank <= previous {
                return Err(format!(
                    "flag `{c}` is out of canonical `{}` order",
                    contract.canonical_order.iter().collect::<String>()
                ));
            }
        }
        last_rank = Some(rank);
        seen.push(c);
    }
    Ok(())
}

fn render_chars(chars: &[char]) -> String {
    chars
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// REGEX pattern
// ---------------------------------------------------------------------------

/// The frozen closed LCL REGEX 0.1.0 grammar.
pub(crate) fn regex_pattern(pattern: &str) -> Result<(), String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut parser = RegexParser {
        chars: &chars,
        pos: 0,
    };
    parser.alternation()?;
    match parser.peek() {
        None => Ok(()),
        Some(')') => Err("`)` without an open group".to_string()),
        Some(c) => Err(format!("unexpected `{c}`")),
    }
}

/// Backslash escapes admitted by `pattern_profiles.REGEX.escape.literal`.
const REGEX_LITERAL_ESCAPES: &str = ".^$|?*+()[]{}\\/-";
/// `escape.control`.
const REGEX_CONTROL_ESCAPES: &str = "nrt";
/// `escape.classes`.
const REGEX_CLASS_ESCAPES: &str = "dwsDWS";

enum ClassMember {
    /// Denotes exactly one scalar; may be a range endpoint.
    Scalar(char),
    /// A class escape such as `\d`; never a range endpoint.
    Class,
}

struct RegexParser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl RegexParser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos.saturating_add(ahead)).copied()
    }

    fn advance(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n);
    }

    /// `ALTERNATION = CONCATENATION , { "|" , CONCATENATION } ;`
    fn alternation(&mut self) -> Result<(), String> {
        self.concatenation()?;
        while self.peek() == Some('|') {
            self.advance(1);
            self.concatenation()?;
        }
        Ok(())
    }

    /// `CONCATENATION = { PIECE } ;`
    fn concatenation(&mut self) -> Result<(), String> {
        loop {
            match self.peek() {
                None | Some('|') | Some(')') => return Ok(()),
                Some(_) => self.piece()?,
            }
        }
    }

    /// `PIECE = ASSERTION | ATOM , [ QUANTIFIER ] ;`
    fn piece(&mut self) -> Result<(), String> {
        if let Some(c @ ('^' | '$')) = self.peek() {
            self.advance(1);
            if self.at_quantifier() {
                return Err(format!("assertion `{c}` cannot be quantified"));
            }
            return Ok(());
        }
        self.atom()?;
        if self.at_quantifier() {
            self.quantifier()?;
            if self.at_quantifier() {
                return Err("more than one quantifier on an atom".to_string());
            }
        }
        Ok(())
    }

    fn at_quantifier(&self) -> bool {
        matches!(self.peek(), Some('*' | '+' | '?' | '{'))
    }

    /// `QUANTIFIER = "*" | "+" | "?" | "{" , COUNT , "}" | "{" , COUNT , "," , [ COUNT ] , "}" ;`
    fn quantifier(&mut self) -> Result<(), String> {
        match self.peek() {
            Some('*' | '+' | '?') => {
                self.advance(1);
                Ok(())
            }
            Some('{') => {
                self.advance(1);
                let n = self.count()?;
                match self.peek() {
                    Some('}') => {
                        self.advance(1);
                        Ok(())
                    }
                    Some(',') => {
                        self.advance(1);
                        if self.peek() == Some('}') {
                            self.advance(1);
                            return Ok(());
                        }
                        let m = self.count()?;
                        if self.peek() != Some('}') {
                            return Err("counted quantifier is not closed by `}`".to_string());
                        }
                        self.advance(1);
                        if count_cmp(&n, &m) == Ordering::Greater {
                            return Err(format!("counted quantifier {{{n},{m}}} requires n <= m"));
                        }
                        Ok(())
                    }
                    _ => Err("counted quantifier is not closed by `}`".to_string()),
                }
            }
            _ => Err("quantifier expected".to_string()),
        }
    }

    /// `COUNT = "0" | NONZERO_DIGIT , { DIGIT } ;`
    ///
    /// `pattern_profiles.REGEX.quantifiers`: "Counts are exact nonnegative
    /// integers without leading zeroes." The production admits an unbounded
    /// digit run, so the count is kept as its canonical decimal digits and
    /// never narrowed to a fixed-width integer; see [`count_cmp`].
    fn count(&mut self) -> Result<String, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.advance(1);
        }
        let digits: String = self.chars[start..self.pos].iter().collect();
        if digits.is_empty() {
            return Err("quantifier count expected".to_string());
        }
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(format!("quantifier count `{digits}` has a leading zero"));
        }
        Ok(digits)
    }

    /// `ATOM = REGEX_LITERAL | ESCAPE | "." | CHARACTER_CLASS | "(" , ALTERNATION , ")" | "(?:" , ALTERNATION , ")" ;`
    fn atom(&mut self) -> Result<(), String> {
        let Some(c) = self.peek() else {
            return Err("atom expected".to_string());
        };
        match c {
            '(' => {
                if self.peek_at(1) == Some('?') {
                    if self.peek_at(2) == Some(':') {
                        self.advance(3);
                    } else {
                        return Err("`(?` other than the non-capturing `(?:` is not admitted (lookaround, named, conditional and inline-flag groups are invalid)".to_string());
                    }
                } else {
                    self.advance(1);
                }
                self.alternation()?;
                if self.peek() == Some(')') {
                    self.advance(1);
                    Ok(())
                } else {
                    Err("unclosed group".to_string())
                }
            }
            ')' => Err("`)` without an open group".to_string()),
            '[' => self.class(),
            '.' => {
                self.advance(1);
                Ok(())
            }
            '\\' => self.escape().map(|_| ()),
            '*' | '+' | '?' | '{' => Err(format!("quantifier `{c}` without an atom")),
            '}' | ']' => Err(format!("`{c}` must be escaped to be literal")),
            _ => {
                self.advance(1);
                Ok(())
            }
        }
    }

    /// One complete escape admitted by the escape table. Positioned at `\`.
    fn escape(&mut self) -> Result<ClassMember, String> {
        let Some(c) = self.peek_at(1) else {
            return Err("trailing backslash".to_string());
        };
        self.advance(2);
        if REGEX_LITERAL_ESCAPES.contains(c) {
            Ok(ClassMember::Scalar(c))
        } else if REGEX_CONTROL_ESCAPES.contains(c) {
            Ok(ClassMember::Scalar(match c {
                'n' => '\n',
                'r' => '\r',
                _ => '\t',
            }))
        } else if REGEX_CLASS_ESCAPES.contains(c) {
            Ok(ClassMember::Class)
        } else {
            Err(format!("`\\{c}` is not an admitted escape"))
        }
    }

    /// `character_class`: `[`, optional `^`, one or more members, `]`.
    fn class(&mut self) -> Result<(), String> {
        self.advance(1);
        if self.peek() == Some('^') {
            self.advance(1);
        }
        let mut members = 0usize;
        loop {
            let Some(c) = self.peek() else {
                return Err("unclosed character class".to_string());
            };
            if c == ']' {
                if members == 0 {
                    return Err("empty character class".to_string());
                }
                self.advance(1);
                return Ok(());
            }
            if c == '-' && members > 0 && !matches!(self.peek_at(1), Some(']')) {
                return Err("unescaped `-` is literal only first or last in a class".to_string());
            }
            let low = self.class_member()?;
            members = members.saturating_add(1);
            if self.peek() == Some('-') && !matches!(self.peek_at(1), Some(']') | None) {
                self.advance(1);
                let ClassMember::Scalar(lo) = low else {
                    return Err("a class escape cannot be a range endpoint".to_string());
                };
                if self.peek() == Some('-') {
                    return Err("unescaped `-` cannot be a range endpoint".to_string());
                }
                let ClassMember::Scalar(hi) = self.class_member()? else {
                    return Err("a class escape cannot be a range endpoint".to_string());
                };
                if lo > hi {
                    return Err(format!("descending range `{lo}-{hi}`"));
                }
            }
        }
    }

    fn class_member(&mut self) -> Result<ClassMember, String> {
        match self.peek() {
            None => Err("unclosed character class".to_string()),
            Some('[') => Err("`[` inside a class must be escaped; classes do not nest".to_string()),
            Some('\\') => self.escape(),
            Some(c) => {
                self.advance(1);
                Ok(ClassMember::Scalar(c))
            }
        }
    }
}

/// Order two `COUNT` lexemes exactly, with no fixed-width limit.
///
/// `COUNT = "0" | NONZERO_DIGIT , { DIGIT }` admits an unbounded digit run, and
/// `pattern_profiles.REGEX.quantifiers` requires `{n,m}` to satisfy `n <= m`
/// over exact nonnegative integers. Both arguments have already passed the
/// no-leading-zero rule of [`RegexParser::count`], so each is the canonical
/// decimal spelling of its value: a longer digit sequence is the larger number,
/// and equal lengths compare ASCII-lexicographically. Narrowing to `u64` would
/// collapse every value above `u64::MAX` onto one saturated bound and lose the
/// ordering of distinct large counts.
fn count_cmp(n: &str, m: &str) -> Ordering {
    n.len().cmp(&m.len()).then_with(|| n.cmp(m))
}

// ---------------------------------------------------------------------------
// GLOB pattern
// ---------------------------------------------------------------------------

/// Characters a backslash may escape in a GLOB (`pattern_profiles.GLOB.escape`).
const GLOB_ESCAPES: &str = "*?[]\\{}!^-";

/// The closed LCL GLOB 0.1.0 profile.
pub(crate) fn glob_pattern(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("pattern must have at least one nonempty segment".to_string());
    }
    if pattern.starts_with('/') {
        return Err("leading slash is invalid".to_string());
    }
    if pattern.ends_with('/') {
        return Err("trailing slash is invalid".to_string());
    }
    for segment in pattern.split('/') {
        if segment.is_empty() {
            return Err("empty segment".to_string());
        }
        if segment == "." || segment == ".." {
            return Err(format!("literal `{segment}` segment is invalid"));
        }
        if segment == "**" {
            continue;
        }
        glob_segment(&segment.chars().collect::<Vec<char>>())?;
    }
    Ok(())
}

fn glob_segment(chars: &[char]) -> Result<(), String> {
    let mut at = 0usize;
    let mut previous_star = false;
    while let Some(&c) = chars.get(at) {
        match c {
            '*' => {
                if previous_star {
                    return Err(
                        "adjacent stars: `**` is legal only as one whole segment".to_string()
                    );
                }
                previous_star = true;
                at = at.saturating_add(1);
                continue;
            }
            '?' => {}
            '[' => {
                at = glob_class(chars, at)?;
                previous_star = false;
                continue;
            }
            ']' | '{' | '}' => return Err(format!("`{c}` must be escaped to be literal")),
            '\\' => {
                let Some(&next) = chars.get(at.saturating_add(1)) else {
                    return Err("trailing backslash".to_string());
                };
                if !GLOB_ESCAPES.contains(next) {
                    return Err(format!("`\\{next}` is not an admitted escape"));
                }
                at = at.saturating_add(2);
                previous_star = false;
                continue;
            }
            _ => {}
        }
        previous_star = false;
        at = at.saturating_add(1);
    }
    Ok(())
}

/// Positioned at `[`; returns the index after the closing `]`.
fn glob_class(chars: &[char], open: usize) -> Result<usize, String> {
    let mut at = open.saturating_add(1);
    if chars.get(at) == Some(&'!') {
        at = at.saturating_add(1);
    }
    let mut members = 0usize;
    loop {
        let Some(&c) = chars.get(at) else {
            return Err("unclosed character class".to_string());
        };
        if c == ']' {
            if members == 0 {
                return Err("empty character class".to_string());
            }
            return Ok(at.saturating_add(1));
        }
        if c == '-' && members > 0 && chars.get(at.saturating_add(1)) != Some(&']') {
            return Err("unescaped `-` is literal only first or last in a class".to_string());
        }
        let (lo, next) = glob_class_member(chars, at)?;
        at = next;
        members = members.saturating_add(1);
        if chars.get(at) == Some(&'-')
            && !matches!(chars.get(at.saturating_add(1)), Some(']') | None)
        {
            at = at.saturating_add(1);
            if chars.get(at) == Some(&'-') {
                return Err("unescaped `-` cannot be a range endpoint".to_string());
            }
            let (hi, next) = glob_class_member(chars, at)?;
            if lo > hi {
                return Err(format!("descending range `{lo}-{hi}`"));
            }
            at = next;
        }
    }
}

fn glob_class_member(chars: &[char], at: usize) -> Result<(char, usize), String> {
    match chars.get(at) {
        None => Err("unclosed character class".to_string()),
        Some('/') => Err("`/` cannot occur in a class".to_string()),
        Some('[') => Err("`[` inside a class must be escaped; classes do not nest".to_string()),
        Some('\\') => {
            let Some(&next) = chars.get(at.saturating_add(1)) else {
                return Err("trailing backslash".to_string());
            };
            if !GLOB_ESCAPES.contains(next) {
                return Err(format!("`\\{next}` is not an admitted escape"));
            }
            Ok((next, at.saturating_add(2)))
        }
        Some(&c) => Ok((c, at.saturating_add(1))),
    }
}

// ---------------------------------------------------------------------------
// Temporal literals
// ---------------------------------------------------------------------------

fn two_digits(b: &[u8], at: usize) -> Option<u32> {
    let hi = *b.get(at)?;
    let lo = *b.get(at.saturating_add(1))?;
    if hi.is_ascii_digit() && lo.is_ascii_digit() {
        Some(u32::from(hi - b'0') * 10 + u32::from(lo - b'0'))
    } else {
        None
    }
}

/// "Exactly YYYY-MM-DD with year 0001..9999, month 01..12, and a valid day in
/// the proleptic Gregorian calendar."
pub(crate) fn date(text: &str) -> Result<(), String> {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !b[..4].iter().all(u8::is_ascii_digit) {
        return Err("DATE must be exactly YYYY-MM-DD".to_string());
    }
    let year: u32 = text[..4].parse().unwrap_or(0);
    let (Some(month), Some(day)) = (two_digits(b, 5), two_digits(b, 8)) else {
        return Err("DATE must be exactly YYYY-MM-DD".to_string());
    };
    if year == 0 {
        return Err("DATE year must be 0001..9999".to_string());
    }
    if !(1..=12).contains(&month) {
        return Err("DATE month must be 01..12".to_string());
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    if day == 0 || day > days {
        return Err(format!("DATE day must be 01..{days:02} for that month"));
    }
    Ok(())
}

/// "Exactly HH:MM:SS with optional . followed by one or more decimal digits,
/// then optional Z or signed HH:MM offset." Leap second 60 and `-00:00` are
/// excluded; only uppercase `Z`.
pub(crate) fn time(text: &str) -> Result<(), String> {
    let b = text.as_bytes();
    let (Some(hours), Some(minutes), Some(seconds)) =
        (two_digits(b, 0), two_digits(b, 3), two_digits(b, 6))
    else {
        return Err("TIME must begin with exactly HH:MM:SS".to_string());
    };
    if b.get(2) != Some(&b':') || b.get(5) != Some(&b':') {
        return Err("TIME must begin with exactly HH:MM:SS".to_string());
    }
    if hours > 23 || minutes > 59 || seconds > 59 {
        return Err("TIME requires hours 00..23, minutes 00..59, seconds 00..59".to_string());
    }
    let mut at = 8usize;
    if b.get(at) == Some(&b'.') {
        at = at.saturating_add(1);
        let digits_start = at;
        while matches!(b.get(at), Some(c) if c.is_ascii_digit()) {
            at = at.saturating_add(1);
        }
        if at == digits_start {
            return Err("TIME fraction requires one or more digits after `.`".to_string());
        }
    }
    match b.get(at) {
        None => Ok(()),
        Some(b'Z') if at.saturating_add(1) == b.len() => Ok(()),
        Some(sign @ (b'+' | b'-')) => {
            let (Some(oh), Some(om)) = (two_digits(b, at + 1), two_digits(b, at + 4)) else {
                return Err("TIME offset must be signed HH:MM".to_string());
            };
            if b.get(at + 3) != Some(&b':') || at + 6 != b.len() {
                return Err("TIME offset must be signed HH:MM".to_string());
            }
            if oh > 23 || om > 59 {
                return Err("TIME offset requires hours 00..23 and minutes 00..59".to_string());
            }
            if *sign == b'-' && oh == 0 && om == 0 {
                return Err("`-00:00` denotes an unknown offset and is excluded".to_string());
            }
            Ok(())
        }
        Some(_) => Err("TIME may end only with `Z` or a signed HH:MM offset".to_string()),
    }
}

/// "Exactly DATE spelling, uppercase T, and TIME spelling under this profile."
pub(crate) fn datetime(text: &str) -> Result<(), String> {
    let b = text.as_bytes();
    if b.len() < 11 || b[10] != b'T' {
        return Err("DATETIME must be DATE, uppercase `T`, then TIME".to_string());
    }
    date(&text[..10])?;
    time(&text[11..])
}

// ---------------------------------------------------------------------------
// URI
// ---------------------------------------------------------------------------

/// RFC 3986 `absolute-URI = scheme ":" hier-part [ "?" query ]`, with a scheme
/// and without a fragment; every component checked against its ABNF character
/// set with `%HH` percent-encoding.
pub(crate) fn uri(text: &str) -> Result<(), String> {
    if !text.is_ascii() {
        return Err("URI must be ASCII (RFC 3986)".to_string());
    }
    let b = text.as_bytes();
    let Some(colon) = b.iter().position(|&c| c == b':') else {
        return Err("absolute-URI requires a scheme followed by `:`".to_string());
    };
    let scheme = &b[..colon];
    let scheme_ok = scheme.first().is_some_and(u8::is_ascii_alphabetic)
        && scheme
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'));
    if !scheme_ok {
        return Err("scheme must be ALPHA *( ALPHA / DIGIT / \"+\" / \"-\" / \".\" )".to_string());
    }
    let rest = &b[colon + 1..];
    if rest.contains(&b'#') {
        return Err("absolute-URI admits no fragment".to_string());
    }
    let (hier, query) = match rest.iter().position(|&c| c == b'?') {
        Some(q) => (&rest[..q], Some(&rest[q + 1..])),
        None => (rest, None),
    };
    if let Some(query) = query {
        check_chars(query, "query", |c| {
            is_pchar_byte(c) || matches!(c, b'/' | b'?')
        })?;
    }
    if let Some(after) = hier.strip_prefix(b"//") {
        let path_start = after.iter().position(|&c| c == b'/').unwrap_or(after.len());
        uri_authority(&after[..path_start])?;
        uri_path_segments(&after[path_start..], false)
    } else if hier.is_empty() {
        Ok(())
    } else if hier[0] == b'/' {
        // path-absolute = "/" [ segment-nz *( "/" segment ) ]
        let tail = &hier[1..];
        if tail.first() == Some(&b'/') {
            return Err("path-absolute cannot begin with `//`".to_string());
        }
        uri_path_segments(tail, true)
    } else {
        // path-rootless = segment-nz *( "/" segment )
        uri_path_segments(hier, true)
    }
}

fn uri_path_segments(path: &[u8], first_nonempty: bool) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    for (index, segment) in path.split(|&c| c == b'/').enumerate() {
        if index == 0 && first_nonempty && segment.is_empty() {
            return Err("path requires a nonempty first segment".to_string());
        }
        check_chars(segment, "path segment", is_pchar_byte)?;
    }
    Ok(())
}

fn uri_authority(authority: &[u8]) -> Result<(), String> {
    let (userinfo, hostport) = match authority.iter().rposition(|&c| c == b'@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    if let Some(userinfo) = userinfo {
        check_chars(userinfo, "userinfo", |c| {
            is_unreserved(c) || is_sub_delim(c) || c == b':' || c == b'%'
        })?;
        check_percent_encoding(userinfo)?;
    }
    let (host, port) = if hostport.first() == Some(&b'[') {
        let Some(close) = hostport.iter().position(|&c| c == b']') else {
            return Err("IP-literal host is not closed by `]`".to_string());
        };
        uri_ip_literal(&hostport[1..close])?;
        let rest = &hostport[close + 1..];
        match rest.first() {
            None => (None, None),
            Some(b':') => (None, Some(&rest[1..])),
            Some(_) => return Err("unexpected characters after IP-literal host".to_string()),
        }
    } else {
        match hostport.iter().rposition(|&c| c == b':') {
            Some(at) => (Some(&hostport[..at]), Some(&hostport[at + 1..])),
            None => (Some(hostport), None),
        }
    };
    if let Some(host) = host {
        check_chars(host, "host", |c| {
            is_unreserved(c) || is_sub_delim(c) || c == b'%'
        })?;
        check_percent_encoding(host)?;
    }
    if let Some(port) = port {
        if !port.iter().all(u8::is_ascii_digit) {
            return Err("port must be digits only".to_string());
        }
    }
    Ok(())
}

/// `IP-literal = "[" ( IPv6address / IPvFuture ) "]"`.
fn uri_ip_literal(inner: &[u8]) -> Result<(), String> {
    if let Some(future) = inner
        .strip_prefix(b"v")
        .or_else(|| inner.strip_prefix(b"V"))
    {
        // IPvFuture = "v" 1*HEXDIG "." 1*( unreserved / sub-delims / ":" )
        let Some(dot) = future.iter().position(|&c| c == b'.') else {
            return Err("IPvFuture requires `.` after its version".to_string());
        };
        if dot == 0 || !future[..dot].iter().all(u8::is_ascii_hexdigit) {
            return Err("IPvFuture version must be 1*HEXDIG".to_string());
        }
        let tail = &future[dot + 1..];
        if tail.is_empty() {
            return Err("IPvFuture requires an address after `.`".to_string());
        }
        return check_chars(tail, "IPvFuture", |c| {
            is_unreserved(c) || is_sub_delim(c) || c == b':'
        });
    }
    uri_ipv6(inner)
}

fn uri_ipv6(text: &[u8]) -> Result<(), String> {
    let s = std::str::from_utf8(text).unwrap_or("");
    let (head, tail, compressed) = match s.find("::") {
        Some(at) => {
            if s[at + 2..].contains("::") {
                return Err("IPv6 address has more than one `::`".to_string());
            }
            (&s[..at], &s[at + 2..], true)
        }
        None => (s, "", false),
    };
    let mut groups = 0usize;
    for (side, part) in [(head, "head"), (tail, "tail")] {
        if side.is_empty() {
            continue;
        }
        let pieces: Vec<&str> = side.split(':').collect();
        for (index, piece) in pieces.iter().enumerate() {
            let last = index + 1 == pieces.len();
            if last && piece.contains('.') && (part == "tail" || !compressed) {
                uri_ipv4(piece)?;
                groups = groups.saturating_add(2);
                continue;
            }
            if piece.is_empty() || piece.len() > 4 || !piece.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!("IPv6 group `{piece}` must be 1..4 hex digits"));
            }
            groups = groups.saturating_add(1);
        }
    }
    if compressed && groups > 7 {
        return Err("IPv6 address with `::` has too many groups".to_string());
    }
    if !compressed && groups != 8 {
        return Err("IPv6 address without `::` must have exactly 8 groups".to_string());
    }
    Ok(())
}

fn uri_ipv4(text: &str) -> Result<(), String> {
    let octets: Vec<&str> = text.split('.').collect();
    if octets.len() != 4 {
        return Err("IPv4 address must have four octets".to_string());
    }
    for octet in octets {
        let ok = !octet.is_empty()
            && octet.len() <= 3
            && octet.bytes().all(|c| c.is_ascii_digit())
            && (octet.len() == 1 || !octet.starts_with('0'))
            && octet.parse::<u32>().is_ok_and(|v| v <= 255);
        if !ok {
            return Err(format!(
                "IPv4 octet `{octet}` must be 0..255 without leading zeros"
            ));
        }
    }
    Ok(())
}

fn is_unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(c: u8) -> bool {
    matches!(
        c,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn is_pchar_byte(c: u8) -> bool {
    is_unreserved(c) || is_sub_delim(c) || matches!(c, b':' | b'@' | b'%')
}

fn check_chars(bytes: &[u8], what: &str, allowed: impl Fn(u8) -> bool) -> Result<(), String> {
    if let Some(&bad) = bytes.iter().find(|&&c| !allowed(c)) {
        return Err(format!(
            "`{}` is not permitted in a URI {what}",
            bad as char
        ));
    }
    check_percent_encoding(bytes)
}

fn check_percent_encoding(bytes: &[u8]) -> Result<(), String> {
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let ok = bytes.get(at + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(at + 2).is_some_and(u8::is_ascii_hexdigit);
            if !ok {
                return Err("`%` must be followed by two hexadecimal digits".to_string());
            }
            at = at.saturating_add(3);
        } else {
            at = at.saturating_add(1);
        }
    }
    Ok(())
}
