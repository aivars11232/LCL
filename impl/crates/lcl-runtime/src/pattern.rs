//! The two closed pattern profiles, executed.
//!
//! Authority: `types_v0.1.0.json#/pattern_profiles`.
//!
//! ## Why an automaton and not a backtracker
//!
//! The `REGEX` profile forbids "lookbehind, unicode_property_escapes,
//! lookahead, backreferences, named_groups, inline_flags, conditional_groups,
//! reluctant_quantifiers, possessive_quantifiers, unlisted_escapes", and says
//! what remains:
//!
//! > Matching is a Boolean relation over a sequence of Unicode scalars. ...
//! > Repetition has the mathematical finite-concatenation meaning even when its
//! > atom accepts empty text; no implementation strategy, capture state, or
//! > greedy/reluctant preference changes Boolean acceptance.
//!
//! A language with no backreferences is regular, and the contract explicitly
//! refuses to let an implementation strategy affect the answer. So this module
//! compiles to a Thompson automaton and simulates a *set* of states. A
//! backtracker would give the same answers but could take exponential time on
//! an adversarial pattern — and this runtime must be total.
//!
//! ## Full-string, without anchors
//!
//! > For REGEX, full-string behavior is a semantic boundary check and is not
//! > expressed by adding start/end anchors, because multiline mode would weaken
//! > that substitution.
//!
//! So acceptance requires a derivation consuming the whole input, and `^`/`$`
//! remain independent assertions whose meaning the `m` flag changes. Wrapping
//! the pattern in `^(?:...)$` — the usual shortcut — would be wrong here, and
//! is not done.
//!
//! ## Bounded
//!
//! Compilation and simulation both carry budgets. Exceeding one produces
//! [`PatternFault::ResourceLimit`], which the caller reports as the registered
//! `error.pattern.resource_limit` rather than hanging or panicking.

use std::collections::BTreeSet;

/// The declared finite resource limits.
///
/// Sized so that every pattern the canonical corpus contains compiles far
/// inside them, while an adversarial pattern is refused rather than run.
const MAX_STATES: usize = 20_000;
const MAX_STEPS: usize = 5_000_000;

/// Why a pattern could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternFault {
    /// The pattern is not admitted by the closed profile. The lexer normally
    /// rejects this first; reaching it here means a dynamically supplied
    /// pattern, which is `error.literal.invalid` under
    /// `expression_demand_resolution`.
    Invalid(String),
    /// "Compiling or matching a demanded, well-typed GLOB or REGEX exhausts its
    /// declared finite pattern-resource limit."
    ResourceLimit(String),
}

// ---------------------------------------------------------------------------
// REGEX
// ---------------------------------------------------------------------------

/// The three admitted flags. `unicode_text_handling` is
/// `always_enabled_independently_of_user_flags`, so there is no Unicode flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// ASCII case-insensitive comparison only.
    pub i: bool,
    /// `^` also accepts a position immediately after U+000A; `$` also accepts a
    /// position immediately before one.
    pub m: bool,
    /// Dot accepts every Unicode scalar.
    pub s: bool,
}

impl Flags {
    /// Parse the flag string. `duplicate_flags_allowed` and
    /// `unknown_flags_allowed` are both false.
    pub fn parse(text: &str) -> Result<Flags, PatternFault> {
        let mut flags = Flags::default();
        for c in text.chars() {
            let slot = match c {
                'i' => &mut flags.i,
                'm' => &mut flags.m,
                's' => &mut flags.s,
                other => {
                    return Err(PatternFault::Invalid(format!(
                        "{other:?} is not one of the admitted flags i, m, s"
                    )))
                }
            };
            if *slot {
                return Err(PatternFault::Invalid(format!("duplicate flag {c:?}")));
            }
            *slot = true;
        }
        Ok(flags)
    }
}

/// One member of a character class.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ClassMember {
    Scalar(char),
    /// An inclusive ascending scalar range.
    Range(char, char),
    /// One of the escape classes `\d`, `\w`, `\s` and their complements.
    Escape(EscapeClass),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeClass {
    Digit,
    NotDigit,
    Word,
    NotWord,
    Space,
    NotSpace,
}

impl EscapeClass {
    fn admits(self, c: char) -> bool {
        // "\d denotes ASCII 0-9; \w denotes ASCII A-Z, a-z, 0-9, and
        // underscore; \s denotes exactly U+0009 through U+000D and U+0020."
        let digit = c.is_ascii_digit();
        let word = c.is_ascii_alphanumeric() || c == '_';
        let space = matches!(c, '\u{9}'..='\u{D}' | '\u{20}');
        match self {
            EscapeClass::Digit => digit,
            EscapeClass::NotDigit => !digit,
            EscapeClass::Word => word,
            EscapeClass::NotWord => !word,
            EscapeClass::Space => space,
            EscapeClass::NotSpace => !space,
        }
    }
}

/// What one consuming transition accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Matcher {
    Scalar(char),
    /// "Without s dot excludes exactly U+000A, U+000D, U+2028, and U+2029."
    Dot,
    Class {
        negated: bool,
        members: Vec<ClassMember>,
    },
}

impl Matcher {
    fn admits(&self, c: char, flags: Flags) -> bool {
        match self {
            Matcher::Scalar(expected) => scalar_eq(*expected, c, flags.i),
            Matcher::Dot => flags.s || !matches!(c, '\u{A}' | '\u{D}' | '\u{2028}' | '\u{2029}'),
            Matcher::Class { negated, members } => {
                // "For classes, first close the positive member set under these
                // pairs, then apply any class negation."
                let positive = members.iter().any(|m| match m {
                    ClassMember::Scalar(s) => scalar_eq(*s, c, flags.i),
                    ClassMember::Range(low, high) => {
                        in_range(*low, *high, c)
                            || (flags.i && case_pair(c).is_some_and(|p| in_range(*low, *high, p)))
                    }
                    ClassMember::Escape(class) => class.admits(c),
                });
                // "Negation complements the union of members over Unicode
                // scalars."
                positive != *negated
            }
        }
    }
}

fn in_range(low: char, high: char, c: char) -> bool {
    low <= c && c <= high
}

/// The ASCII case partner of a scalar, if it has one.
///
/// "ASCII case-insensitive comparison only: A-Z and a-z are equivalent pairs.
/// ... Other Unicode scalars remain distinct; no locale or external Unicode
/// case-folding table participates."
fn case_pair(c: char) -> Option<char> {
    if c.is_ascii_uppercase() {
        Some(c.to_ascii_lowercase())
    } else if c.is_ascii_lowercase() {
        Some(c.to_ascii_uppercase())
    } else {
        None
    }
}

fn scalar_eq(expected: char, actual: char, insensitive: bool) -> bool {
    expected == actual
        || (insensitive && case_pair(expected).is_some_and(|paired| paired == actual))
}

/// A zero-width assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assertion {
    Start,
    End,
}

/// One automaton state.
#[derive(Debug, Clone)]
enum State {
    /// Consume one scalar, then continue.
    Consume(Matcher, usize),
    /// Epsilon to both targets.
    Split(usize, usize),
    /// Zero-width assertion, then continue.
    Assert(Assertion, usize),
    /// The single accepting state.
    Accept,
}

/// A compiled pattern.
#[derive(Debug, Clone)]
pub struct Regex {
    states: Vec<State>,
    start: usize,
    flags: Flags,
}

impl Regex {
    /// Compile one pattern under the closed profile.
    pub fn compile(pattern: &str, flags: Flags) -> Result<Regex, PatternFault> {
        let scalars: Vec<char> = pattern.chars().collect();
        let mut parser = Parser {
            input: &scalars,
            position: 0,
        };
        let node = parser.alternation()?;
        if parser.position != scalars.len() {
            return Err(PatternFault::Invalid(format!(
                "unexpected {:?} at scalar {}",
                scalars[parser.position], parser.position
            )));
        }
        let mut builder = Builder { states: Vec::new() };
        let accept = builder.push(State::Accept)?;
        let start = builder.compile(&node, accept)?;
        Ok(Regex {
            states: builder.states,
            start,
            flags,
        })
    }

    /// True when the pattern accepts the whole input.
    ///
    /// "Success requires one derivation consuming the entire input from its
    /// first boundary through its final boundary."
    pub fn matches(&self, input: &str) -> Result<bool, PatternFault> {
        let scalars: Vec<char> = input.chars().collect();
        let mut steps = 0usize;
        let mut current: BTreeSet<usize> = BTreeSet::new();
        self.close(self.start, 0, &scalars, &mut current, &mut steps)?;

        for (index, scalar) in scalars.iter().enumerate() {
            let mut next: BTreeSet<usize> = BTreeSet::new();
            for state in &current {
                steps += 1;
                if steps > MAX_STEPS {
                    return Err(PatternFault::ResourceLimit(
                        "matching exceeded the declared step limit".to_string(),
                    ));
                }
                if let Some(State::Consume(matcher, target)) = self.states.get(*state) {
                    if matcher.admits(*scalar, self.flags) {
                        self.close(*target, index + 1, &scalars, &mut next, &mut steps)?;
                    }
                }
            }
            if next.is_empty() {
                return Ok(false);
            }
            current = next;
        }

        Ok(current
            .iter()
            .any(|s| matches!(self.states.get(*s), Some(State::Accept))))
    }

    /// Epsilon closure at one input position, evaluating assertions there.
    fn close(
        &self,
        state: usize,
        position: usize,
        input: &[char],
        out: &mut BTreeSet<usize>,
        steps: &mut usize,
    ) -> Result<(), PatternFault> {
        let mut stack = vec![state];
        while let Some(state) = stack.pop() {
            *steps += 1;
            if *steps > MAX_STEPS {
                return Err(PatternFault::ResourceLimit(
                    "closure exceeded the declared step limit".to_string(),
                ));
            }
            if !out.insert(state) {
                continue;
            }
            match self.states.get(state) {
                Some(State::Split(a, b)) => {
                    stack.push(*a);
                    stack.push(*b);
                }
                Some(State::Assert(assertion, target))
                    if self.holds(*assertion, position, input) =>
                {
                    stack.push(*target);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn holds(&self, assertion: Assertion, position: usize, input: &[char]) -> bool {
        match assertion {
            // "Without m they accept only the start and end respectively."
            Assertion::Start => {
                position == 0 || (self.flags.m && input.get(position - 1) == Some(&'\n'))
            }
            Assertion::End => {
                position == input.len() || (self.flags.m && input.get(position) == Some(&'\n'))
            }
        }
    }
}

/// The pattern syntax tree.
#[derive(Debug, Clone)]
enum Node {
    Empty,
    Atom(Matcher),
    Assert(Assertion),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
    },
}

struct Parser<'a> {
    input: &'a [char],
    position: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.input.get(self.position).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += 1;
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    /// `ALTERNATION = CONCATENATION , { "|" , CONCATENATION }`
    fn alternation(&mut self) -> Result<Node, PatternFault> {
        let mut branches = vec![self.concatenation()?];
        while self.eat('|') {
            branches.push(self.concatenation()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap_or(Node::Empty)
        } else {
            Node::Alt(branches)
        })
    }

    /// `CONCATENATION = { PIECE }`
    fn concatenation(&mut self) -> Result<Node, PatternFault> {
        let mut pieces = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            pieces.push(self.piece()?);
        }
        Ok(match pieces.len() {
            // "Empty concatenation accepts the empty substring."
            0 => Node::Empty,
            1 => pieces.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(pieces),
        })
    }

    /// `PIECE = ASSERTION | ATOM , [ QUANTIFIER ]`
    fn piece(&mut self) -> Result<Node, PatternFault> {
        // "An assertion cannot be quantified."
        if self.eat('^') {
            return Ok(Node::Assert(Assertion::Start));
        }
        if self.eat('$') {
            return Ok(Node::Assert(Assertion::End));
        }
        let atom = self.atom()?;
        match self.quantifier()? {
            Some((min, max)) => {
                // "More than one quantifier on an atom is invalid."
                if self.quantifier()?.is_some() {
                    return Err(PatternFault::Invalid(
                        "an atom carries more than one quantifier".to_string(),
                    ));
                }
                Ok(Node::Repeat {
                    node: Box::new(atom),
                    min,
                    max,
                })
            }
            None => Ok(atom),
        }
    }

    fn atom(&mut self) -> Result<Node, PatternFault> {
        let Some(c) = self.peek() else {
            return Err(PatternFault::Invalid("expected an atom".to_string()));
        };
        match c {
            '(' => {
                self.position += 1;
                // "(?:" is the only admitted group prefix; inline flags and
                // named groups are forbidden features.
                if self.peek() == Some('?') {
                    if self.input.get(self.position + 1) != Some(&':') {
                        return Err(PatternFault::Invalid(
                            "only the non-capturing group prefix (?: is admitted".to_string(),
                        ));
                    }
                    self.position += 2;
                }
                let inner = self.alternation()?;
                if !self.eat(')') {
                    return Err(PatternFault::Invalid("unclosed group".to_string()));
                }
                Ok(inner)
            }
            '[' => {
                self.position += 1;
                Ok(Node::Atom(self.character_class()?))
            }
            '.' => {
                self.position += 1;
                Ok(Node::Atom(Matcher::Dot))
            }
            '\\' => {
                self.position += 1;
                Ok(match self.escape()? {
                    Escape::Scalar(c) => Node::Atom(Matcher::Scalar(c)),
                    Escape::Class(class) => Node::Atom(Matcher::Class {
                        negated: false,
                        members: vec![ClassMember::Escape(class)],
                    }),
                })
            }
            // REGEX_LITERAL excludes the metacharacters.
            '*' | '+' | '?' | ')' | ']' | '{' | '}' | '|' | '^' | '$' => Err(
                PatternFault::Invalid(format!("{c:?} is not a literal in this position")),
            ),
            other => {
                self.position += 1;
                Ok(Node::Atom(Matcher::Scalar(other)))
            }
        }
    }

    fn quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, PatternFault> {
        match self.peek() {
            Some('*') => {
                self.position += 1;
                Ok(Some((0, None)))
            }
            Some('+') => {
                self.position += 1;
                Ok(Some((1, None)))
            }
            Some('?') => {
                self.position += 1;
                Ok(Some((0, Some(1))))
            }
            Some('{') => {
                let saved = self.position;
                self.position += 1;
                let Some(min) = self.count() else {
                    self.position = saved;
                    return Err(PatternFault::Invalid(
                        "a repetition count must be an exact nonnegative integer".to_string(),
                    ));
                };
                if self.eat('}') {
                    return Ok(Some((min, Some(min))));
                }
                if !self.eat(',') {
                    return Err(PatternFault::Invalid("malformed repetition".to_string()));
                }
                if self.eat('}') {
                    return Ok(Some((min, None)));
                }
                let Some(max) = self.count() else {
                    return Err(PatternFault::Invalid("malformed repetition".to_string()));
                };
                if !self.eat('}') {
                    return Err(PatternFault::Invalid("unclosed repetition".to_string()));
                }
                // "{n,m} from n through m inclusive with n <= m."
                if min > max {
                    return Err(PatternFault::Invalid(
                        "a repetition range must be ascending".to_string(),
                    ));
                }
                Ok(Some((min, Some(max))))
            }
            _ => Ok(None),
        }
    }

    /// `COUNT = "0" | NONZERO_DIGIT , { DIGIT }` — "without leading zeroes".
    fn count(&mut self) -> Option<u32> {
        let start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.position += 1;
        }
        if start == self.position {
            return None;
        }
        let digits: String = self.input[start..self.position].iter().collect();
        if digits.len() > 1 && digits.starts_with('0') {
            return None;
        }
        digits.parse().ok()
    }

    fn character_class(&mut self) -> Result<Matcher, PatternFault> {
        // "A class is [ followed by optional ^ negation, one or more members,
        // and ]."
        let negated = self.eat('^');
        let mut members: Vec<ClassMember> = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(PatternFault::Invalid(
                    "unclosed character class".to_string(),
                ));
            };
            if c == ']' {
                self.position += 1;
                break;
            }
            // "Unescaped - is literal only first or last."
            let member = if c == '\\' {
                self.position += 1;
                match self.escape()? {
                    Escape::Scalar(scalar) => ClassMember::Scalar(scalar),
                    Escape::Class(class) => ClassMember::Escape(class),
                }
            } else if c == '[' {
                return Err(PatternFault::Invalid(
                    "a literal [ inside a class requires escaping".to_string(),
                ));
            } else {
                self.position += 1;
                ClassMember::Scalar(c)
            };
            // A range needs a literal endpoint on both sides.
            if self.peek() == Some('-') && self.input.get(self.position + 1) != Some(&']') {
                let ClassMember::Scalar(low) = member else {
                    return Err(PatternFault::Invalid(
                        "a class escape cannot be a range endpoint".to_string(),
                    ));
                };
                self.position += 1;
                let high = match self.peek() {
                    Some('\\') => {
                        self.position += 1;
                        match self.escape()? {
                            Escape::Scalar(scalar) => scalar,
                            Escape::Class(_) => {
                                return Err(PatternFault::Invalid(
                                    "a class escape cannot be a range endpoint".to_string(),
                                ))
                            }
                        }
                    }
                    Some(other) if other != ']' => {
                        self.position += 1;
                        other
                    }
                    _ => {
                        return Err(PatternFault::Invalid(
                            "a range requires an upper endpoint".to_string(),
                        ))
                    }
                };
                // "inclusive ascending scalar range"
                if high < low {
                    return Err(PatternFault::Invalid(
                        "a class range must be ascending".to_string(),
                    ));
                }
                members.push(ClassMember::Range(low, high));
            } else {
                members.push(member);
            }
        }
        if members.is_empty() {
            return Err(PatternFault::Invalid(
                "an empty class is invalid".to_string(),
            ));
        }
        Ok(Matcher::Class { negated, members })
    }

    fn escape(&mut self) -> Result<Escape, PatternFault> {
        let Some(c) = self.bump() else {
            return Err(PatternFault::Invalid("a trailing backslash".to_string()));
        };
        Ok(match c {
            // "A backslash followed by exactly one of . ^ $ | ? * + ( ) [ ] { }
            // backslash / or - denotes that literal scalar."
            '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '\\'
            | '/' | '-' => Escape::Scalar(c),
            // "\n is U+000A, \r is U+000D, and \t is U+0009."
            'n' => Escape::Scalar('\u{A}'),
            'r' => Escape::Scalar('\u{D}'),
            't' => Escape::Scalar('\u{9}'),
            'd' => Escape::Class(EscapeClass::Digit),
            'D' => Escape::Class(EscapeClass::NotDigit),
            'w' => Escape::Class(EscapeClass::Word),
            'W' => Escape::Class(EscapeClass::NotWord),
            's' => Escape::Class(EscapeClass::Space),
            'S' => Escape::Class(EscapeClass::NotSpace),
            // "Every unlisted escape, including numeric backreferences, \b, \u,
            // \x, \p, and \P, is invalid."
            other => {
                return Err(PatternFault::Invalid(format!(
                    "\\{other} is not an admitted escape"
                )))
            }
        })
    }
}

enum Escape {
    Scalar(char),
    Class(EscapeClass),
}

struct Builder {
    states: Vec<State>,
}

impl Builder {
    fn push(&mut self, state: State) -> Result<usize, PatternFault> {
        if self.states.len() >= MAX_STATES {
            return Err(PatternFault::ResourceLimit(
                "compiling exceeded the declared state limit".to_string(),
            ));
        }
        self.states.push(state);
        Ok(self.states.len() - 1)
    }

    /// Compile `node` so that it continues at `next`, returning its entry.
    fn compile(&mut self, node: &Node, next: usize) -> Result<usize, PatternFault> {
        Ok(match node {
            Node::Empty => next,
            Node::Atom(matcher) => self.push(State::Consume(matcher.clone(), next))?,
            Node::Assert(assertion) => self.push(State::Assert(*assertion, next))?,
            Node::Concat(nodes) => {
                let mut entry = next;
                for node in nodes.iter().rev() {
                    entry = self.compile(node, entry)?;
                }
                entry
            }
            Node::Alt(branches) => {
                let mut entry: Option<usize> = None;
                for branch in branches.iter().rev() {
                    let compiled = self.compile(branch, next)?;
                    entry = Some(match entry {
                        None => compiled,
                        Some(other) => self.push(State::Split(compiled, other))?,
                    });
                }
                entry.unwrap_or(next)
            }
            Node::Repeat { node, min, max } => self.repeat(node, *min, *max, next)?,
        })
    }

    /// Expand a repetition to its "mathematical finite-concatenation meaning".
    fn repeat(
        &mut self,
        node: &Node,
        min: u32,
        max: Option<u32>,
        next: usize,
    ) -> Result<usize, PatternFault> {
        match max {
            // Unbounded tail: a loop. `Split` makes both "take the loop" and
            // "leave" reachable, which is acceptance, not preference.
            None => {
                let split = self.push(State::Split(next, next))?;
                let body = self.compile(node, split)?;
                self.states[split] = State::Split(body, next);
                let mut entry = split;
                for _ in 0..min {
                    entry = self.compile(node, entry)?;
                }
                Ok(entry)
            }
            Some(max) => {
                // The optional tail, innermost last.
                let mut entry = next;
                for _ in min..max {
                    let body = self.compile(node, entry)?;
                    entry = self.push(State::Split(body, next))?;
                }
                // The required prefix.
                for _ in 0..min {
                    entry = self.compile(node, entry)?;
                }
                Ok(entry)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GLOB
// ---------------------------------------------------------------------------

/// A compiled `GLOB` under `closed_lcl_glob_0_1_0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// "** consumes zero or more complete nonempty segments."
    AnySegments,
    Ordinary(Vec<GlobToken>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GlobToken {
    Scalar(char),
    /// "zero_or_more_non_separator_characters"
    Star,
    /// "one_non_separator_character"
    Question,
    /// "one_non_separator_character_from_class"
    Class {
        negated: bool,
        members: Vec<ClassMember>,
    },
}

impl Glob {
    /// Compile one pattern under the closed profile.
    pub fn compile(pattern: &str) -> Result<Glob, PatternFault> {
        // "Leading/trailing slash, empty segments, literal . or .. segments ...
        // are invalid." "absolute_patterns_allowed": false.
        if pattern.is_empty() {
            return Err(PatternFault::Invalid(
                "an empty GLOB is invalid".to_string(),
            ));
        }
        if pattern.starts_with('/') || pattern.ends_with('/') {
            return Err(PatternFault::Invalid(
                "a GLOB has no leading or trailing slash".to_string(),
            ));
        }
        let mut segments = Vec::new();
        for raw in pattern.split('/') {
            if raw.is_empty() {
                return Err(PatternFault::Invalid("an empty segment".to_string()));
            }
            if raw == "." || raw == ".." {
                return Err(PatternFault::Invalid(format!(
                    "the literal segment {raw:?} is invalid"
                )));
            }
            if raw == "**" {
                segments.push(Segment::AnySegments);
                continue;
            }
            if raw.contains("**") {
                return Err(PatternFault::Invalid(
                    "** is legal only as one whole segment".to_string(),
                ));
            }
            segments.push(Segment::Ordinary(Glob::tokens(raw)?));
        }
        Ok(Glob { segments })
    }

    fn tokens(segment: &str) -> Result<Vec<GlobToken>, PatternFault> {
        let scalars: Vec<char> = segment.chars().collect();
        let mut parser = Parser {
            input: &scalars,
            position: 0,
        };
        let mut tokens = Vec::new();
        let mut previous_star = false;
        while let Some(c) = parser.peek() {
            let token = match c {
                '*' => {
                    // "adjacent stars in an ordinary segment are invalid"
                    if previous_star {
                        return Err(PatternFault::Invalid(
                            "adjacent stars in an ordinary segment".to_string(),
                        ));
                    }
                    parser.position += 1;
                    GlobToken::Star
                }
                '?' => {
                    parser.position += 1;
                    GlobToken::Question
                }
                '[' => {
                    parser.position += 1;
                    match parser.glob_class()? {
                        Matcher::Class { negated, members } => {
                            GlobToken::Class { negated, members }
                        }
                        _ => unreachable!("glob_class returns a class"),
                    }
                }
                '\\' => {
                    parser.position += 1;
                    // "A backslash escapes exactly one of * ? [ ] backslash { }
                    // ! ^ or -."
                    let Some(escaped) = parser.bump() else {
                        return Err(PatternFault::Invalid("a trailing backslash".to_string()));
                    };
                    if !matches!(
                        escaped,
                        '*' | '?' | '[' | ']' | '\\' | '{' | '}' | '!' | '^' | '-'
                    ) {
                        return Err(PatternFault::Invalid(format!(
                            "\\{escaped} is not an admitted GLOB escape"
                        )));
                    }
                    GlobToken::Scalar(escaped)
                }
                ']' | '{' | '}' => {
                    return Err(PatternFault::Invalid(format!(
                        "{c:?} requires escaping in a GLOB"
                    )))
                }
                other => {
                    parser.position += 1;
                    GlobToken::Scalar(other)
                }
            };
            previous_star = matches!(token, GlobToken::Star);
            tokens.push(token);
        }
        if tokens.is_empty() {
            return Err(PatternFault::Invalid("an empty segment".to_string()));
        }
        Ok(tokens)
    }

    /// True when the pattern consumes the complete input segment sequence.
    pub fn matches(&self, input: &str) -> Result<bool, PatternFault> {
        // "A STRING operand denotes either the empty relative path or nonempty
        // slash-separated segments."
        let segments: Vec<&str> = if input.is_empty() {
            Vec::new()
        } else {
            input.split('/').collect()
        };
        if segments.iter().any(|s| s.is_empty()) {
            return Ok(false);
        }
        let mut steps = 0usize;
        Glob::match_segments(&self.segments, &segments, &mut steps)
    }

    fn match_segments(
        pattern: &[Segment],
        input: &[&str],
        steps: &mut usize,
    ) -> Result<bool, PatternFault> {
        *steps += 1;
        if *steps > MAX_STEPS {
            return Err(PatternFault::ResourceLimit(
                "GLOB matching exceeded the declared step limit".to_string(),
            ));
        }
        match pattern.split_first() {
            None => Ok(input.is_empty()),
            Some((Segment::AnySegments, rest)) => {
                // "** consumes zero or more complete nonempty segments."
                for taken in 0..=input.len() {
                    if Glob::match_segments(rest, &input[taken..], steps)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Some((Segment::Ordinary(tokens), rest)) => {
                let Some((head, tail)) = input.split_first() else {
                    return Ok(false);
                };
                let scalars: Vec<char> = head.chars().collect();
                if !Glob::match_tokens(tokens, &scalars, steps)? {
                    return Ok(false);
                }
                Glob::match_segments(rest, tail, steps)
            }
        }
    }

    fn match_tokens(
        tokens: &[GlobToken],
        input: &[char],
        steps: &mut usize,
    ) -> Result<bool, PatternFault> {
        *steps += 1;
        if *steps > MAX_STEPS {
            return Err(PatternFault::ResourceLimit(
                "GLOB matching exceeded the declared step limit".to_string(),
            ));
        }
        match tokens.split_first() {
            None => Ok(input.is_empty()),
            Some((GlobToken::Star, rest)) => {
                for taken in 0..=input.len() {
                    if Glob::match_tokens(rest, &input[taken..], steps)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Some((token, rest)) => {
                let Some((head, tail)) = input.split_first() else {
                    return Ok(false);
                };
                // A class or ? never matches the separator, which cannot occur
                // inside a segment anyway.
                let admitted = match token {
                    GlobToken::Scalar(expected) => expected == head,
                    GlobToken::Question => *head != '/',
                    GlobToken::Class { negated, members } => {
                        let positive = members.iter().any(|m| match m {
                            ClassMember::Scalar(s) => s == head,
                            ClassMember::Range(low, high) => in_range(*low, *high, *head),
                            ClassMember::Escape(class) => class.admits(*head),
                        });
                        // "Negation complements the class over Unicode scalars
                        // excluding /."
                        (positive != *negated) && *head != '/'
                    }
                    GlobToken::Star => unreachable!("handled above"),
                };
                if !admitted {
                    return Ok(false);
                }
                Glob::match_tokens(rest, tail, steps)
            }
        }
    }
}

impl<'a> Parser<'a> {
    /// A GLOB class, whose negation marker is `!` rather than `^`.
    fn glob_class(&mut self) -> Result<Matcher, PatternFault> {
        let negated = self.eat('!');
        let mut members: Vec<ClassMember> = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(PatternFault::Invalid(
                    "unclosed character class".to_string(),
                ));
            };
            if c == ']' {
                self.position += 1;
                break;
            }
            if c == '/' {
                return Err(PatternFault::Invalid(
                    "/ cannot occur in a GLOB class".to_string(),
                ));
            }
            let low = if c == '\\' {
                self.position += 1;
                let Some(escaped) = self.bump() else {
                    return Err(PatternFault::Invalid("a trailing backslash".to_string()));
                };
                escaped
            } else if c == '[' {
                return Err(PatternFault::Invalid(
                    "[ is not a nested-class opener and must be escaped".to_string(),
                ));
            } else {
                self.position += 1;
                c
            };
            if self.peek() == Some('-') && self.input.get(self.position + 1) != Some(&']') {
                self.position += 1;
                let Some(high) = self.bump() else {
                    return Err(PatternFault::Invalid(
                        "a range requires an upper endpoint".to_string(),
                    ));
                };
                if high < low {
                    return Err(PatternFault::Invalid(
                        "a class range must be ascending".to_string(),
                    ));
                }
                members.push(ClassMember::Range(low, high));
            } else {
                members.push(ClassMember::Scalar(low));
            }
        }
        if members.is_empty() {
            return Err(PatternFault::Invalid(
                "an empty class is invalid".to_string(),
            ));
        }
        Ok(Matcher::Class { negated, members })
    }
}
