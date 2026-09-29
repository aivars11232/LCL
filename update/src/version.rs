//! Product versions, compared by Semantic Versioning 2.0.0 precedence and
//! nothing looser: `0.10.0` is newer than `0.9.0`, `1.0.0-rc.1` is older than
//! `1.0.0`. Text is never compared as text. Build metadata (`+...`) is refused
//! rather than ignored, so two different version texts never compare equal.
//!
//! This is the LCL *product* version, which every binary and the Android app
//! of one release report. It is not the LCL Core language version.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pre: Vec<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Identifier {
    Numeric(u64),
    Alphanumeric(String),
}

impl Version {
    pub fn parse(text: &str) -> Result<Version, String> {
        let bad = || {
            format!(
                "{text:?} is not a product version: MAJOR.MINOR.PATCH, optionally followed \
                 by -PRE-RELEASE"
            )
        };
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let numbers: Vec<&str> = core.split('.').collect();
        if numbers.len() != 3 {
            return Err(bad());
        }
        let number = |part: &str| -> Result<u64, String> {
            let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            if !digits || (part.len() > 1 && part.starts_with('0')) {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|id| {
                    let allowed = id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
                    if id.is_empty() || !allowed {
                        return Err(bad());
                    }
                    if id.bytes().all(|b| b.is_ascii_digit()) {
                        if id.len() > 1 && id.starts_with('0') {
                            return Err(bad());
                        }
                        id.parse().map(Identifier::Numeric).map_err(|_| bad())
                    } else {
                        Ok(Identifier::Alphanumeric(id.to_string()))
                    }
                })
                .collect::<Result<_, _>>()?,
        };
        Ok(Version {
            major: number(numbers[0])?,
            minor: number(numbers[1])?,
            patch: number(numbers[2])?,
            pre,
        })
    }

    /// Whether this is a pre-release, which the stable channel never offers.
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// The version as people read it in LCL's own words: a release's one
    /// trailing `.0` dropped, so `0.5.0` reads `0.5` and `1.2.0` reads `1.2`,
    /// while `1.2.3` and every pre-release read in full. For text only:
    /// comparisons, tags, file names and manifests keep the full version.
    pub fn shown(&self) -> String {
        if self.patch == 0 && self.pre.is_empty() {
            format!("{}.{}", self.major, self.minor)
        } else {
            self.to_string()
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let order = match (a, b) {
                            (Identifier::Numeric(a), Identifier::Numeric(b)) => a.cmp(b),
                            (Identifier::Numeric(_), Identifier::Alphanumeric(_)) => Ordering::Less,
                            (Identifier::Alphanumeric(_), Identifier::Numeric(_)) => {
                                Ordering::Greater
                            }
                            (Identifier::Alphanumeric(a), Identifier::Alphanumeric(b)) => {
                                a.as_bytes().cmp(b.as_bytes())
                            }
                        };
                        if order != Ordering::Equal {
                            return order;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        for (i, id) in self.pre.iter().enumerate() {
            f.write_str(if i == 0 { "-" } else { "." })?;
            match id {
                Identifier::Numeric(n) => write!(f, "{n}")?,
                Identifier::Alphanumeric(s) => f.write_str(s)?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn precedence_is_semantic_versioning_never_text() {
        let ordered = [
            "0.9.0",
            "0.10.0",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.2.3",
            "1.10.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert_eq!(v("1.2.3").cmp(&v("1.2.3")), Ordering::Equal);
        assert_eq!(v("1.0.0-rc.1").to_string(), "1.0.0-rc.1");
        assert!(v("1.0.0-rc.1").is_prerelease() && !v("1.0.0").is_prerelease());
    }

    #[test]
    fn a_release_reads_without_one_trailing_zero_and_nothing_else_is_shortened() {
        for (full, shown) in [
            ("0.5.0", "0.5"),
            ("1.2.0", "1.2"),
            ("1.0.0", "1.0"),
            ("1.2.3", "1.2.3"),
            ("0.10.0", "0.10"),
            ("1.0.0-rc.1", "1.0.0-rc.1"),
            ("1.2.0-beta.0", "1.2.0-beta.0"),
        ] {
            assert_eq!(v(full).shown(), shown, "{full}");
            // What is shown never replaces what is compared.
            assert_eq!(v(full).to_string(), full);
        }
    }

    #[test]
    fn the_published_updater_accepts_and_orders_the_next_release() {
        // v0.1.1 carries exactly this parser: 0.5.0 is a version to it, and a
        // newer one, while 0.5 is not a version at all.
        assert!(v("0.5.0") > v("0.1.1"));
        assert!(Version::parse("0.5").is_err());
    }

    #[test]
    fn anything_that_is_not_exactly_a_version_is_refused() {
        for text in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "01.2.3",
            "1.02.3",
            "1.2.3+build",
            "1.2.3-",
            "1.2.3-01",
            "1.2.3-a..b",
            "1.2.3-a_b",
            " 1.2.3",
            "1.2.-3",
            "18446744073709551616.0.0",
        ] {
            assert!(Version::parse(text).is_err(), "{text:?} was accepted");
        }
    }
}
