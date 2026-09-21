//! The closed GLOB and REGEX matching profiles.
//!
//! `03_TYPES_AND_VALUES/07`: "GLOB and REGEX are frozen LCL 0.1.0 languages
//! defined completely by `10_REGISTRIES/types_v0.1.0.json#/pattern_profiles`. …
//! Similarity to another pattern language does not import additional syntax,
//! matching behavior, or a version-dependent feature."
//!
//! ## Why matching lives here
//!
//! M1 already validates pattern *syntax*: a malformed pattern or flag string is
//! `error.literal.invalid` at the lexical stage and never reaches this crate.
//! What is left is *acceptance* — whether a statically known value satisfies a
//! declared `PATTERN` constraint — which is `error.pattern.mismatch`, registered
//! at `static_or_expression`. Only a statically known value is judged here; a
//! value known solely at demand is recorded as an obligation instead.
//!
//! ## Bounded by construction
//!
//! REGEX matching is an NFA state-set simulation, so it visits at most
//! `states × (input length + 1)` configurations and no pattern can make it
//! backtrack exponentially. GLOB matching is a memoized segment walk with the
//! same guarantee. Both count their steps against [`STEP_LIMIT`] and report
//! [`PatternError::ResourceLimit`] rather than running longer, which is the
//! registered `error.pattern.resource_limit` outcome.

use std::collections::{BTreeMap, BTreeSet};

/// Which closed profile a pattern belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PatternKind {
    Glob,
    Regex,
}

/// The declared finite resource limit this implementation applies to matching.
const STEP_LIMIT: u64 = 2_000_000;

/// The declared finite resource limit this implementation applies to
/// compilation. A bounded repeat is expanded literally, so a count beyond this
/// exhausts the budget rather than the machine.
const STATE_LIMIT: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternError {
    /// The pattern is not well formed under its profile. M1 owns this for
    /// source literals; reaching it here means the text did not come from a
    /// validated literal.
    Malformed,
    /// `error.pattern.resource_limit`.
    ResourceLimit,
}

// ---------------------------------------------------------------------------
// Shared character classes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClassItem {
    Scalar(char),
    Range(char, char),
    /// `\d`, `\w`, `\s` and their complements, which are REGEX-only.
    Shorthand(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Class {
    negated: bool,
    items: Vec<ClassItem>,
    /// GLOB classes never admit the separator, whatever they list.
    exclude_separator: bool,
}

impl Class {
    fn admits(&self, value: char, fold_ascii_case: bool) -> bool {
        if self.exclude_separator && value == '/' {
            return false;
        }
        let mut hit = self.items.iter().any(|item| item.admits(value));
        if !hit && fold_ascii_case {
            // "For a class, close the positive member set under those pairs
            // before applying negation."
            let swapped = if value.is_ascii_lowercase() {
                value.to_ascii_uppercase()
            } else if value.is_ascii_uppercase() {
                value.to_ascii_lowercase()
            } else {
                value
            };
            hit = swapped != value && self.items.iter().any(|item| item.admits(swapped));
        }
        hit != self.negated
    }
}

impl ClassItem {
    fn admits(&self, value: char) -> bool {
        match self {
            ClassItem::Scalar(c) => *c == value,
            ClassItem::Range(low, high) => *low <= value && value <= *high,
            ClassItem::Shorthand(kind) => shorthand_admits(*kind, value),
        }
    }
}

/// `\d` ASCII digits, `\w` ASCII letters, digits and underscore, `\s` exactly
/// U+0009 through U+000D and U+0020; uppercase spellings are their complements.
fn shorthand_admits(kind: char, value: char) -> bool {
    match kind {
        'd' => value.is_ascii_digit(),
        'w' => value.is_ascii_alphanumeric() || value == '_',
        's' => matches!(value, '\u{9}'..='\u{d}' | ' '),
        'D' => !value.is_ascii_digit(),
        'W' => !(value.is_ascii_alphanumeric() || value == '_'),
        'S' => !matches!(value, '\u{9}'..='\u{d}' | ' '),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// REGEX
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Empty,
    Scalar(char),
    Class(Class),
    /// "Dot excludes U+000A, U+000D, U+2028, and U+2029 unless s is present."
    Dot,
    Start,
    End,
    Concat(Vec<Node>),
    Alternate(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
    },
}

/// One compiled pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pattern {
    kind: PatternKind,
    regex: Option<CompiledRegex>,
    glob: Option<Vec<GlobSegment>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledRegex {
    states: Vec<State>,
    start: usize,
    accept: usize,
    ignore_ascii_case: bool,
    multiline: bool,
    dot_all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// Consume one scalar admitted by the matcher, then continue.
    Consume(Consumer, usize),
    /// Continue to both targets without consuming.
    Split(usize, usize),
    /// Continue only where the assertion holds.
    Assert(Assertion, usize),
    Accept,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Consumer {
    Scalar(char),
    Class(Class),
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assertion {
    Start,
    End,
}

/// Whether one closed-profile `GLOB` selects one workspace-relative subject.
///
/// `types_v0.1.0.json#/pattern_profiles/GLOB`: `workspace_relative: true`,
/// `match_semantics: full_workspace_relative_path`. The subject is the
/// normalized relative segment sequence its caller derived; this function
/// neither infers a root nor reads a filesystem.
///
/// `None` means the pattern could not be decided — malformed text, which an
/// earlier stage owns as `error.literal.invalid`, or the declared finite
/// resource limit. A caller that cannot decide a restriction must not treat it
/// as absent.
pub fn glob_selects(pattern: &str, relative: &str) -> Option<bool> {
    compile(PatternKind::Glob, pattern, "")
        .and_then(|compiled| compiled.matches(relative))
        .ok()
}

/// Whether one closed-profile `REGEX` selects one subject.
///
/// `types_v0.1.0.json#/pattern_profiles/REGEX`: `match_semantics: full_string`.
pub fn regex_selects(pattern: &str, flags: &str, subject: &str) -> Option<bool> {
    compile(PatternKind::Regex, pattern, flags)
        .and_then(|compiled| compiled.matches(subject))
        .ok()
}

/// Compile one pattern of the named profile.
pub(crate) fn compile(
    kind: PatternKind,
    pattern: &str,
    flags: &str,
) -> Result<Pattern, PatternError> {
    match kind {
        PatternKind::Regex => {
            let node = RegexParser::new(pattern).parse()?;
            let mut builder = Builder {
                states: Vec::new(),
                exhausted: false,
            };
            let accept = builder.push(State::Accept);
            let start = builder.build(&node, accept);
            if builder.exhausted {
                return Err(PatternError::ResourceLimit);
            }
            Ok(Pattern {
                kind,
                regex: Some(CompiledRegex {
                    states: builder.states,
                    start,
                    accept,
                    ignore_ascii_case: flags.contains('i'),
                    multiline: flags.contains('m'),
                    dot_all: flags.contains('s'),
                }),
                glob: None,
            })
        }
        PatternKind::Glob => Ok(Pattern {
            kind,
            regex: None,
            glob: Some(parse_glob(pattern)?),
        }),
    }
}

impl Pattern {
    /// True when the whole input is accepted.
    ///
    /// "REGEX matching succeeds iff one derivation consumes the complete input
    /// between its first and final boundaries." A GLOB "whole pattern must
    /// consume the whole relative segment sequence."
    pub(crate) fn matches(&self, input: &str) -> Result<bool, PatternError> {
        match (&self.regex, &self.glob) {
            (Some(regex), _) => regex.matches(input),
            (_, Some(segments)) => glob_matches(segments, input),
            _ => Err(PatternError::Malformed),
        }
    }
}

struct Builder {
    states: Vec<State>,
    /// Set when the compiled program would exceed [`STATE_LIMIT`]. The program
    /// is then never run; the compile reports its resource limit instead.
    exhausted: bool,
}

impl Builder {
    fn push(&mut self, state: State) -> usize {
        if self.states.len() >= STATE_LIMIT {
            self.exhausted = true;
            return 0;
        }
        self.states.push(state);
        self.states.len() - 1
    }

    /// Build the fragment for `node` continuing at `next`, returning its entry.
    fn build(&mut self, node: &Node, next: usize) -> usize {
        match node {
            Node::Empty => next,
            Node::Scalar(c) => self.push(State::Consume(Consumer::Scalar(*c), next)),
            Node::Class(class) => self.push(State::Consume(Consumer::Class(class.clone()), next)),
            Node::Dot => self.push(State::Consume(Consumer::Dot, next)),
            Node::Start => self.push(State::Assert(Assertion::Start, next)),
            Node::End => self.push(State::Assert(Assertion::End, next)),
            Node::Concat(nodes) => {
                let mut entry = next;
                for item in nodes.iter().rev() {
                    entry = self.build(item, entry);
                }
                entry
            }
            Node::Alternate(branches) => {
                let mut entry = None;
                for branch in branches {
                    let built = self.build(branch, next);
                    entry = Some(match entry {
                        None => built,
                        Some(previous) => self.push(State::Split(built, previous)),
                    });
                }
                entry.unwrap_or(next)
            }
            Node::Repeat { node, min, max } => self.repeat(node, *min, *max, next),
        }
    }

    /// "Repetition means finite concatenation mathematically, including when its
    /// atom accepts empty text; no greediness or implementation strategy changes
    /// Boolean acceptance." So a bounded repeat is expanded literally, and an
    /// unbounded one becomes a loop.
    fn repeat(&mut self, node: &Node, min: u32, max: Option<u32>, next: usize) -> usize {
        match max {
            None => {
                // `min` copies, then a loop over the atom.
                if self.states.len() >= STATE_LIMIT {
                    self.exhausted = true;
                    return next;
                }
                let loop_split = self.states.len();
                self.states.push(State::Split(0, next));
                if self.exhausted {
                    return next;
                }
                let body = self.build(node, loop_split);
                if let Some(slot) = self.states.get_mut(loop_split) {
                    *slot = State::Split(body, next);
                }
                let mut entry = loop_split;
                for _ in 0..min {
                    entry = self.build(node, entry);
                }
                entry
            }
            Some(max) => {
                if max as usize > STATE_LIMIT {
                    self.exhausted = true;
                    return next;
                }
                let mut entry = next;
                for index in (0..max).rev() {
                    let body = self.build(node, entry);
                    entry = if index < min {
                        body
                    } else {
                        self.push(State::Split(body, next))
                    };
                }
                entry
            }
        }
    }
}

impl CompiledRegex {
    fn matches(&self, input: &str) -> Result<bool, PatternError> {
        let scalars: Vec<char> = input.chars().collect();
        let mut steps: u64 = 0;
        let mut current: BTreeSet<usize> = BTreeSet::new();
        self.closure(&mut current, self.start, 0, &scalars, &mut steps)?;

        for (position, scalar) in scalars.iter().enumerate() {
            let mut next: BTreeSet<usize> = BTreeSet::new();
            for state in &current {
                steps = steps.saturating_add(1);
                if steps > STEP_LIMIT {
                    return Err(PatternError::ResourceLimit);
                }
                if let State::Consume(consumer, target) = &self.states[*state] {
                    if self.admits(consumer, *scalar) {
                        self.closure(&mut next, *target, position + 1, &scalars, &mut steps)?;
                    }
                }
            }
            current = next;
            if current.is_empty() {
                return Ok(false);
            }
        }
        Ok(current.contains(&self.accept))
    }

    fn admits(&self, consumer: &Consumer, scalar: char) -> bool {
        match consumer {
            Consumer::Scalar(expected) => {
                *expected == scalar
                    || (self.ignore_ascii_case && expected.eq_ignore_ascii_case(&scalar))
            }
            Consumer::Class(class) => class.admits(scalar, self.ignore_ascii_case),
            // "Dot excludes U+000A, U+000D, U+2028, and U+2029 unless s is
            // present; with s it accepts every scalar."
            Consumer::Dot => {
                self.dot_all || !matches!(scalar, '\n' | '\r' | '\u{2028}' | '\u{2029}')
            }
        }
    }

    /// Epsilon closure from `state` at `position`, applying assertions.
    fn closure(
        &self,
        out: &mut BTreeSet<usize>,
        state: usize,
        position: usize,
        scalars: &[char],
        steps: &mut u64,
    ) -> Result<(), PatternError> {
        let mut stack = vec![state];
        while let Some(current) = stack.pop() {
            *steps = steps.saturating_add(1);
            if *steps > STEP_LIMIT {
                return Err(PatternError::ResourceLimit);
            }
            if !out.insert(current) {
                continue;
            }
            match &self.states[current] {
                State::Split(a, b) => {
                    stack.push(*a);
                    stack.push(*b);
                }
                State::Assert(assertion, target) => {
                    if self.holds(*assertion, position, scalars) {
                        stack.push(*target);
                    }
                }
                State::Consume(_, _) | State::Accept => {}
            }
        }
        Ok(())
    }

    /// "Without m, ^ and $ assert the input's initial and final boundaries.
    /// With m, they additionally assert positions after and before U+000A
    /// respectively. There is no implicit pre-final-newline match."
    fn holds(&self, assertion: Assertion, position: usize, scalars: &[char]) -> bool {
        match assertion {
            Assertion::Start => {
                position == 0
                    || (self.multiline && position > 0 && scalars.get(position - 1) == Some(&'\n'))
            }
            Assertion::End => {
                position == scalars.len()
                    || (self.multiline && scalars.get(position) == Some(&'\n'))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// REGEX parsing
// ---------------------------------------------------------------------------

struct RegexParser {
    scalars: Vec<char>,
    at: usize,
}

impl RegexParser {
    fn new(pattern: &str) -> RegexParser {
        RegexParser {
            scalars: pattern.chars().collect(),
            at: 0,
        }
    }

    fn parse(&mut self) -> Result<Node, PatternError> {
        let node = self.alternation()?;
        if self.at != self.scalars.len() {
            return Err(PatternError::Malformed);
        }
        Ok(node)
    }

    fn peek(&self) -> Option<char> {
        self.scalars.get(self.at).copied()
    }

    fn alternation(&mut self) -> Result<Node, PatternError> {
        let mut branches = vec![self.concatenation()?];
        while self.peek() == Some('|') {
            self.at += 1;
            branches.push(self.concatenation()?);
        }
        Ok(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Node::Alternate(branches)
        })
    }

    fn concatenation(&mut self) -> Result<Node, PatternError> {
        let mut items = Vec::new();
        while let Some(scalar) = self.peek() {
            if scalar == '|' || scalar == ')' {
                break;
            }
            items.push(self.piece()?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.remove(0),
            _ => Node::Concat(items),
        })
    }

    fn piece(&mut self) -> Result<Node, PatternError> {
        let atom = self.atom()?;
        // "Anchors cannot be quantified."
        let anchor = matches!(atom, Node::Start | Node::End);
        let Some((min, max)) = self.quantifier()? else {
            return Ok(atom);
        };
        if anchor {
            return Err(PatternError::Malformed);
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
        })
    }

    fn quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, PatternError> {
        let bounds = match self.peek() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            Some('{') => return self.counted_quantifier(),
            _ => return Ok(None),
        };
        self.at += 1;
        // "Repeated quantifiers … are invalid."
        if matches!(self.peek(), Some('*') | Some('+') | Some('?') | Some('{')) {
            return Err(PatternError::Malformed);
        }
        Ok(Some(bounds))
    }

    fn counted_quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, PatternError> {
        let start = self.at;
        self.at += 1;
        let min = self.count()?;
        let max = match self.peek() {
            Some('}') => Some(min),
            Some(',') => {
                self.at += 1;
                if self.peek() == Some('}') {
                    None
                } else {
                    Some(self.count()?)
                }
            }
            _ => {
                self.at = start;
                return Err(PatternError::Malformed);
            }
        };
        if self.peek() != Some('}') {
            return Err(PatternError::Malformed);
        }
        self.at += 1;
        // "Counts are exact nonnegative integers without leading zeroes and
        // require n <= m."
        if let Some(max) = max {
            if max < min {
                return Err(PatternError::Malformed);
            }
        }
        if matches!(self.peek(), Some('*') | Some('+') | Some('?') | Some('{')) {
            return Err(PatternError::Malformed);
        }
        Ok(Some((min, max)))
    }

    fn count(&mut self) -> Result<u32, PatternError> {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == start {
            return Err(PatternError::Malformed);
        }
        let text: String = self.scalars[start..self.at].iter().collect();
        if text.len() > 1 && text.starts_with('0') {
            return Err(PatternError::Malformed);
        }
        text.parse::<u32>().map_err(|_| PatternError::ResourceLimit)
    }

    fn atom(&mut self) -> Result<Node, PatternError> {
        let Some(scalar) = self.peek() else {
            return Err(PatternError::Malformed);
        };
        match scalar {
            '^' => {
                self.at += 1;
                Ok(Node::Start)
            }
            '$' => {
                self.at += 1;
                Ok(Node::End)
            }
            '.' => {
                self.at += 1;
                Ok(Node::Dot)
            }
            '(' => {
                self.at += 1;
                // "ordinary (...) or non-capturing (?:...) grouping"
                if self.peek() == Some('?') {
                    if self.scalars.get(self.at + 1).copied() != Some(':') {
                        return Err(PatternError::Malformed);
                    }
                    self.at += 2;
                }
                let inner = self.alternation()?;
                if self.peek() != Some(')') {
                    return Err(PatternError::Malformed);
                }
                self.at += 1;
                Ok(inner)
            }
            '[' => self.class(),
            '\\' => {
                self.at += 1;
                let Some(escaped) = self.peek() else {
                    return Err(PatternError::Malformed);
                };
                self.at += 1;
                Ok(match escaped {
                    'n' => Node::Scalar('\n'),
                    'r' => Node::Scalar('\r'),
                    't' => Node::Scalar('\t'),
                    'd' | 'w' | 's' | 'D' | 'W' | 'S' => Node::Class(Class {
                        negated: false,
                        items: vec![ClassItem::Shorthand(escaped)],
                        exclude_separator: false,
                    }),
                    // "REGEX backslash escapes its metacharacters, slash, and
                    // hyphen literally."
                    '\\' | '.' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '*' | '+' | '?'
                    | '^' | '$' | '/' | '-' => Node::Scalar(escaped),
                    _ => return Err(PatternError::Malformed),
                })
            }
            ')' | ']' | '*' | '+' | '?' | '{' | '}' | '|' => Err(PatternError::Malformed),
            other => {
                self.at += 1;
                Ok(Node::Scalar(other))
            }
        }
    }

    fn class(&mut self) -> Result<Node, PatternError> {
        self.at += 1;
        let negated = if self.peek() == Some('^') {
            self.at += 1;
            true
        } else {
            false
        };
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(scalar) = self.peek() else {
                return Err(PatternError::Malformed);
            };
            if scalar == ']' && !first {
                self.at += 1;
                break;
            }
            if scalar == ']' && first {
                return Err(PatternError::Malformed);
            }
            let member = self.class_member()?;
            // "Unescaped hyphen is literal only first or last."
            if self.peek() == Some('-')
                && self.scalars.get(self.at + 1).copied() != Some(']')
                && self.scalars.get(self.at + 1).is_some()
            {
                self.at += 1;
                let high = self.class_member()?;
                let (ClassItem::Scalar(low), ClassItem::Scalar(high)) = (&member, &high) else {
                    // "Range endpoints each denote one scalar, so class escapes
                    // cannot be endpoints."
                    return Err(PatternError::Malformed);
                };
                if high < low {
                    return Err(PatternError::Malformed);
                }
                items.push(ClassItem::Range(*low, *high));
            } else {
                items.push(member);
            }
            first = false;
        }
        if items.is_empty() {
            return Err(PatternError::Malformed);
        }
        Ok(Node::Class(Class {
            negated,
            items,
            exclude_separator: false,
        }))
    }

    fn class_member(&mut self) -> Result<ClassItem, PatternError> {
        let Some(scalar) = self.peek() else {
            return Err(PatternError::Malformed);
        };
        if scalar == '\\' {
            self.at += 1;
            let Some(escaped) = self.peek() else {
                return Err(PatternError::Malformed);
            };
            self.at += 1;
            return Ok(match escaped {
                'n' => ClassItem::Scalar('\n'),
                'r' => ClassItem::Scalar('\r'),
                't' => ClassItem::Scalar('\t'),
                'd' | 'w' | 's' | 'D' | 'W' | 'S' => ClassItem::Shorthand(escaped),
                '\\' | ']' | '[' | '-' | '^' | '/' | '.' | '*' | '+' | '?' | '(' | ')' | '{'
                | '}' | '|' | '$' => ClassItem::Scalar(escaped),
                _ => return Err(PatternError::Malformed),
            });
        }
        self.at += 1;
        Ok(ClassItem::Scalar(scalar))
    }
}

// ---------------------------------------------------------------------------
// GLOB
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum GlobSegment {
    /// `**` — "consumes zero or more complete nonempty path segments".
    DoubleStar,
    Ordinary(Vec<GlobToken>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GlobToken {
    /// "* consumes zero or more non-separator scalars"
    Star,
    /// "? consumes exactly one"
    Any,
    Scalar(char),
    Class(Class),
}

fn parse_glob(pattern: &str) -> Result<Vec<GlobSegment>, PatternError> {
    if pattern.is_empty() {
        return Err(PatternError::Malformed);
    }
    let mut segments = Vec::new();
    for raw in pattern.split('/') {
        if raw.is_empty() {
            // "Leading/trailing slash, empty segments … are invalid."
            return Err(PatternError::Malformed);
        }
        if raw == "**" {
            segments.push(GlobSegment::DoubleStar);
            continue;
        }
        if raw == "." || raw == ".." {
            return Err(PatternError::Malformed);
        }
        segments.push(GlobSegment::Ordinary(parse_glob_segment(raw)?));
    }
    Ok(segments)
}

fn parse_glob_segment(segment: &str) -> Result<Vec<GlobToken>, PatternError> {
    let scalars: Vec<char> = segment.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0usize;
    while at < scalars.len() {
        match scalars[at] {
            '*' => {
                // "adjacent stars inside an ordinary segment" are invalid.
                if tokens.last() == Some(&GlobToken::Star) {
                    return Err(PatternError::Malformed);
                }
                tokens.push(GlobToken::Star);
                at += 1;
            }
            '?' => {
                tokens.push(GlobToken::Any);
                at += 1;
            }
            '[' => {
                let (class, next) = parse_glob_class(&scalars, at)?;
                tokens.push(GlobToken::Class(class));
                at = next;
            }
            '\\' => {
                at += 1;
                let Some(escaped) = scalars.get(at).copied() else {
                    return Err(PatternError::Malformed);
                };
                if !matches!(
                    escaped,
                    '*' | '?' | '[' | ']' | '\\' | '{' | '}' | '!' | '^' | '-'
                ) {
                    return Err(PatternError::Malformed);
                }
                tokens.push(GlobToken::Scalar(escaped));
                at += 1;
            }
            '{' | '}' => return Err(PatternError::Malformed),
            other => {
                tokens.push(GlobToken::Scalar(other));
                at += 1;
            }
        }
    }
    Ok(tokens)
}

fn parse_glob_class(scalars: &[char], open: usize) -> Result<(Class, usize), PatternError> {
    let mut at = open + 1;
    // "Initial ! complements the class over scalars other than slash."
    let negated = scalars.get(at) == Some(&'!');
    if negated {
        at += 1;
    }
    let mut items = Vec::new();
    let mut first = true;
    loop {
        let Some(scalar) = scalars.get(at).copied() else {
            return Err(PatternError::Malformed);
        };
        if scalar == ']' && !first {
            at += 1;
            break;
        }
        if scalar == ']' && first {
            return Err(PatternError::Malformed);
        }
        if scalar == '/' {
            // "A class cannot contain slash."
            return Err(PatternError::Malformed);
        }
        let member = if scalar == '\\' {
            at += 1;
            let Some(escaped) = scalars.get(at).copied() else {
                return Err(PatternError::Malformed);
            };
            if !matches!(
                escaped,
                '*' | '?' | '[' | ']' | '\\' | '{' | '}' | '!' | '^' | '-'
            ) {
                return Err(PatternError::Malformed);
            }
            at += 1;
            escaped
        } else {
            at += 1;
            scalar
        };
        if scalars.get(at) == Some(&'-')
            && scalars.get(at + 1).is_some()
            && scalars.get(at + 1) != Some(&']')
        {
            at += 1;
            let Some(high) = scalars.get(at).copied() else {
                return Err(PatternError::Malformed);
            };
            if high == '/' {
                return Err(PatternError::Malformed);
            }
            at += 1;
            if high < member {
                return Err(PatternError::Malformed);
            }
            items.push(ClassItem::Range(member, high));
        } else {
            items.push(ClassItem::Scalar(member));
        }
        first = false;
    }
    if items.is_empty() {
        return Err(PatternError::Malformed);
    }
    Ok((
        Class {
            negated,
            items,
            exclude_separator: true,
        },
        at,
    ))
}

/// "A GLOB STRING input is empty for the relative root, or consists of nonempty
/// slash-separated segments without . or .., leading slash, or trailing slash."
fn glob_matches(pattern: &[GlobSegment], input: &str) -> Result<bool, PatternError> {
    let segments: Vec<&str> = if input.is_empty() {
        Vec::new()
    } else {
        input.split('/').collect()
    };
    for segment in &segments {
        if segment.is_empty() || *segment == "." || *segment == ".." {
            return Err(PatternError::Malformed);
        }
    }
    let mut memo: BTreeMap<(usize, usize), bool> = BTreeMap::new();
    let mut steps = 0u64;
    glob_walk(pattern, &segments, 0, 0, &mut memo, &mut steps)
}

fn glob_walk(
    pattern: &[GlobSegment],
    input: &[&str],
    pattern_at: usize,
    input_at: usize,
    memo: &mut BTreeMap<(usize, usize), bool>,
    steps: &mut u64,
) -> Result<bool, PatternError> {
    *steps = steps.saturating_add(1);
    if *steps > STEP_LIMIT {
        return Err(PatternError::ResourceLimit);
    }
    if let Some(known) = memo.get(&(pattern_at, input_at)) {
        return Ok(*known);
    }
    let result = match pattern.get(pattern_at) {
        None => input_at == input.len(),
        Some(GlobSegment::DoubleStar) => {
            let mut hit = false;
            for consumed in input_at..=input.len() {
                if glob_walk(pattern, input, pattern_at + 1, consumed, memo, steps)? {
                    hit = true;
                    break;
                }
            }
            hit
        }
        Some(GlobSegment::Ordinary(tokens)) => match input.get(input_at) {
            None => false,
            Some(segment) => {
                glob_segment_matches(tokens, segment, steps)?
                    && glob_walk(pattern, input, pattern_at + 1, input_at + 1, memo, steps)?
            }
        },
    };
    memo.insert((pattern_at, input_at), result);
    Ok(result)
}

fn glob_segment_matches(
    tokens: &[GlobToken],
    segment: &str,
    steps: &mut u64,
) -> Result<bool, PatternError> {
    let scalars: Vec<char> = segment.chars().collect();
    // Reachable input positions after each token, so a `*` cannot backtrack
    // exponentially.
    let mut positions: BTreeSet<usize> = BTreeSet::new();
    positions.insert(0);
    for token in tokens {
        let mut next = BTreeSet::new();
        for position in &positions {
            *steps = steps.saturating_add(1);
            if *steps > STEP_LIMIT {
                return Err(PatternError::ResourceLimit);
            }
            match token {
                GlobToken::Star => {
                    for reach in *position..=scalars.len() {
                        next.insert(reach);
                    }
                }
                GlobToken::Any => {
                    if *position < scalars.len() {
                        next.insert(position + 1);
                    }
                }
                GlobToken::Scalar(expected) => {
                    if scalars.get(*position) == Some(expected) {
                        next.insert(position + 1);
                    }
                }
                GlobToken::Class(class) => {
                    if let Some(scalar) = scalars.get(*position) {
                        if class.admits(*scalar, false) {
                            next.insert(position + 1);
                        }
                    }
                }
            }
        }
        positions = next;
        if positions.is_empty() {
            return Ok(false);
        }
    }
    Ok(positions.contains(&scalars.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regex(pattern: &str, flags: &str, input: &str) -> bool {
        compile(PatternKind::Regex, pattern, flags)
            .expect("pattern compiles")
            .matches(input)
            .expect("match completes")
    }

    fn glob(pattern: &str, input: &str) -> bool {
        compile(PatternKind::Glob, pattern, "")
            .expect("pattern compiles")
            .matches(input)
            .expect("match completes")
    }

    #[test]
    fn regex_matching_consumes_the_complete_input() {
        // "REGEX matching succeeds iff one derivation consumes the complete
        // input between its first and final boundaries."
        assert!(regex("[A-Za-z]+", "", "Alpha"));
        assert!(!regex("[A-Za-z]+", "", "Alpha1"));
        assert!(!regex("lph", "", "Alpha"));
        assert!(regex("^[a-z]+$", "", "alpha"));
        assert!(!regex("^[a-z]+$", "", "Alpha"));
    }

    #[test]
    fn quantifiers_are_exact_finite_concatenation() {
        assert!(regex("a*", "", ""));
        assert!(regex("a*", "", "aaa"));
        assert!(!regex("a+", "", ""));
        assert!(regex("a?", "", "a"));
        assert!(regex("a{3}", "", "aaa"));
        assert!(!regex("a{3}", "", "aa"));
        assert!(regex("a{2,}", "", "aaaa"));
        assert!(regex("a{2,3}", "", "aaa"));
        assert!(!regex("a{2,3}", "", "aaaa"));
        // "no greediness or implementation strategy changes Boolean acceptance"
        assert!(regex("(a|ab)(c|bc)", "", "abc"));
    }

    #[test]
    fn alternation_grouping_and_classes_follow_the_profile() {
        assert!(regex("cat|dog", "", "dog"));
        assert!(regex("(?:ab)+", "", "abab"));
        assert!(regex("[^a-z]+", "", "123"));
        assert!(regex("[a-]", "", "-"));
        assert!(regex("\\d{2}", "", "42"));
        assert!(regex("\\w+", "", "a_1"));
        assert!(regex("\\s", "", " "));
        assert!(!regex("\\D", "", "4"));
    }

    #[test]
    fn flags_are_exactly_i_m_and_s() {
        // "i equates ASCII A-Z with a-z only."
        assert!(regex("[a-z]+", "i", "ALPHA"));
        assert!(regex("abc", "i", "AbC"));
        // "Dot excludes U+000A … unless s is present."
        assert!(!regex("a.b", "", "a\nb"));
        assert!(regex("a.b", "s", "a\nb"));
        // "With m, they additionally assert positions after and before U+000A
        // respectively" — while "m never relaxes the complete-input
        // requirement", so the whole input must still be consumed.
        assert!(regex("a\\n^b", "m", "a\nb"));
        assert!(!regex("a\\n^b", "", "a\nb"));
        assert!(regex("a$\\nb", "m", "a\nb"));
        assert!(!regex("^b$", "m", "a\nb"));
    }

    #[test]
    fn unlisted_regex_syntax_is_not_admitted() {
        for pattern in [
            "a**", "(?=a)", "a\\b", "\\p{L}", "(a", "[]", "[z-a]", "a{2,1}",
        ] {
            assert_eq!(
                compile(PatternKind::Regex, pattern, "").err(),
                Some(PatternError::Malformed),
                "{pattern} is not admitted"
            );
        }
    }

    #[test]
    fn glob_matches_whole_relative_segment_sequences() {
        // The four examples the profile states verbatim.
        assert!(glob("src/**/*.py", "src/main.py"));
        assert!(glob("**", ""));
        assert!(glob("*", ".hidden"));
        assert!(!glob("a/*", "a/b/c"));

        assert!(glob("src/**/*.py", "src/a/b/main.py"));
        assert!(glob("a/?/c", "a/b/c"));
        assert!(!glob("a/?/c", "a/bb/c"));
        assert!(glob("[abc]*", "b1"));
        assert!(glob("[!abc]*", "z1"));
        // "A class cannot contain slash" and "* consumes zero or more
        // non-separator scalars".
        assert!(!glob("a*", "a/b"));
    }

    #[test]
    fn invalid_glob_shapes_are_not_admitted() {
        for pattern in [
            "/a", "a/", "a//b", "a/./b", "a/../b", "a**b", "a{b}", "[]", "[z-a]",
        ] {
            assert_eq!(
                compile(PatternKind::Glob, pattern, "").err(),
                Some(PatternError::Malformed),
                "{pattern} is not admitted"
            );
        }
    }

    #[test]
    fn a_pattern_beyond_the_declared_budget_reports_its_resource_limit() {
        // "Exhausting a declared finite resource limit while compiling or
        // matching either profile produces error.pattern.resource_limit."
        assert_eq!(
            compile(PatternKind::Regex, "a{999999}", "").err(),
            Some(PatternError::ResourceLimit)
        );
    }

    #[test]
    fn matching_is_linear_and_cannot_be_made_to_backtrack() {
        // A shape that is catastrophic for a backtracking engine completes here.
        let pattern = "(a*)*b";
        let input = "a".repeat(2000);
        assert!(!compile(PatternKind::Regex, pattern, "")
            .expect("compiles")
            .matches(&input)
            .expect("completes"));
    }
}
