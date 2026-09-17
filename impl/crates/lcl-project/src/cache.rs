//! The content-addressed package cache.
//!
//! ## What a cache is for here, and what it is not
//!
//! `04_GRAMMAR/05` requires a `URI` import to carry a `CHECKSUM`, and
//! `07_VERSIONING_AND_EXTENSIONS/02` fixes its form: "Checksum form is
//! algorithm:lowercase_hex; Core 0.1.0 recognizes only sha256." The resolver
//! already verifies that checksum against the bytes it receives, and nothing
//! here changes or repeats that decision.
//!
//! What the cache does is answer *where the bytes come from* when the tool does
//! not fetch. A blob is stored under the digest of its own content, so the
//! store is self-verifying: a file whose contents no longer hash to its name is
//! refused, and the language-level checksum check still runs afterwards on
//! whatever is returned. Two independent checks, neither standing in for the
//! other.
//!
//! ## Why the index is not JSON
//!
//! It is written by the tool, and the format it is written in is the one the
//! canonical package already uses for exactly this job: a digest, two spaces,
//! a name, one entry per line, sorted. That needs no writer library, diffs
//! cleanly, and is verifiable by eye. The hand-written project manifest is
//! JSON, because a human writes it and the trust root already has a strict
//! reader for it.

use lcl_resolver::LoadError;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// The first line of an index file, which fixes its format.
const INDEX_HEADER: &str = "lcl-cache/1";

/// The index file's name inside the cache directory.
const INDEX_FILE: &str = "index";

/// The subdirectory holding blobs, named for the one algorithm Core 0.1.0
/// recognizes.
const BLOBS: &str = "sha256";

/// Why a cache operation failed.
///
/// Never a language diagnostic. A cache is product machinery; a document that
/// cannot be supplied simply fails to resolve, with `error.import.not_found`,
/// which the resolver decides.
#[derive(Debug)]
pub enum CacheError {
    Io {
        path: PathBuf,
        detail: String,
    },
    Malformed {
        path: PathBuf,
        detail: String,
    },
    /// A blob's contents no longer hash to the name it is stored under.
    Corrupt {
        digest: String,
        actual: String,
    },
    /// Text that is not a URI under the `URI` literal profile, which the index
    /// could not hold as exactly one entry and no import could name.
    Uri {
        uri: String,
        detail: String,
    },
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::Io { path, detail } => write!(f, "{}: {detail}", path.display()),
            CacheError::Malformed { path, detail } => {
                write!(f, "{}: {detail}", path.display())
            }
            CacheError::Corrupt { digest, actual } => write!(
                f,
                "the cached blob stored as {digest} now hashes to {actual}"
            ),
            CacheError::Uri { uri, detail } => write!(f, "{uri:?} is not a URI: {detail}"),
        }
    }
}

impl std::error::Error for CacheError {}

/// A local, content-addressed store of imported sources.
#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
    /// URI -> lowercase hex SHA-256, in ascending URI order.
    index: BTreeMap<String, String>,
}

impl Cache {
    /// Open an existing cache directory, or an empty cache if it has no index.
    ///
    /// An absent cache is not an error: it is a project that has vendored
    /// nothing yet. A malformed index *is* an error, because silently treating
    /// a damaged index as empty would turn a corrupted store into a missing
    /// import.
    pub fn open(dir: impl AsRef<Path>) -> Result<Cache, CacheError> {
        let dir = dir.as_ref().to_path_buf();
        let index_path = dir.join(INDEX_FILE);
        if !index_path.exists() {
            return Ok(Cache {
                dir,
                index: BTreeMap::new(),
            });
        }
        let text = crate::read_text(&index_path).map_err(|e| CacheError::Io {
            path: index_path.clone(),
            detail: format!("the cache index is not readable: {e}"),
        })?;
        let index = parse_index(&text).map_err(|detail| CacheError::Malformed {
            path: index_path.clone(),
            detail,
        })?;
        Ok(Cache { dir, index })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every cached source, as URI and digest, in ascending URI order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.index.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The bytes cached for one URI.
    ///
    /// Verifies the blob against the digest it is stored under before returning
    /// it. The importing document's own `CHECKSUM` is checked afterwards, by
    /// the resolver, against these same bytes.
    pub fn get(&self, uri: &str) -> Result<Vec<u8>, LoadError> {
        let Some(digest) = self.index.get(uri) else {
            return Err(LoadError::new(format!(
                "{uri:?} is not in the package cache at {}",
                self.dir.display()
            )));
        };
        let path = self.blob(digest);
        let bytes = crate::read_file(&path).map_err(|e| {
            LoadError::new(format!(
                "the cache lists {uri:?} but its blob is unreadable: {e}"
            ))
        })?;
        let actual = lcl_spec::sha256::hex_digest(&bytes);
        if &actual != digest {
            return Err(LoadError::new(format!(
                "the cached blob for {uri:?} is corrupt: stored as {digest}, hashes to {actual}"
            )));
        }
        Ok(bytes)
    }

    /// Store `bytes` for `uri`, returning the digest they were stored under.
    ///
    /// Writing the same bytes twice is a no-op on the blob and leaves the index
    /// unchanged, because the blob's name *is* its content. Storing different
    /// bytes for a URI that already has an entry replaces the entry: a caller
    /// who vendors a source again is stating that the new bytes are the ones
    /// they mean, and the importing document's `CHECKSUM` decides whether the
    /// language accepts them.
    ///
    /// `uri` must meet the `URI` literal profile, the only form an import can
    /// name. That profile admits no whitespace or control character, so the
    /// index line holding it cannot be split or forged.
    pub fn put(&mut self, uri: &str, bytes: &[u8]) -> Result<String, CacheError> {
        lcl_lexer::uri_profile(uri).map_err(|detail| CacheError::Uri {
            uri: uri.to_string(),
            detail,
        })?;
        let digest = lcl_spec::sha256::hex_digest(bytes);
        let blobs = self.dir.join(BLOBS);
        std::fs::create_dir_all(&blobs).map_err(|e| CacheError::Io {
            path: blobs.clone(),
            detail: format!("the cache directory could not be created: {e}"),
        })?;
        let path = self.blob(&digest);
        if !path.exists() {
            std::fs::write(&path, bytes).map_err(|e| CacheError::Io {
                path: path.clone(),
                detail: format!("the blob could not be written: {e}"),
            })?;
        }
        self.index.insert(uri.to_string(), digest.clone());
        self.write_index()?;
        Ok(digest)
    }

    /// Verify every entry: the blob exists and still hashes to its name.
    pub fn verify(&self) -> Vec<CacheError> {
        let mut faults = Vec::new();
        for (uri, digest) in &self.index {
            let path = self.blob(digest);
            match crate::read_file(&path) {
                Err(e) => faults.push(CacheError::Io {
                    path: path.clone(),
                    detail: format!("the blob for {uri:?} is unreadable: {e}"),
                }),
                Ok(bytes) => {
                    let actual = lcl_spec::sha256::hex_digest(&bytes);
                    if &actual != digest {
                        faults.push(CacheError::Corrupt {
                            digest: digest.clone(),
                            actual,
                        });
                    }
                }
            }
        }
        faults
    }

    fn blob(&self, digest: &str) -> PathBuf {
        self.dir.join(BLOBS).join(digest)
    }

    fn write_index(&self) -> Result<(), CacheError> {
        let mut out = String::from(INDEX_HEADER);
        out.push('\n');
        for (uri, digest) in &self.index {
            out.push_str(digest);
            out.push_str("  ");
            out.push_str(uri);
            out.push('\n');
        }
        let path = self.dir.join(INDEX_FILE);
        std::fs::write(&path, out).map_err(|e| CacheError::Io {
            path,
            detail: format!("the cache index could not be written: {e}"),
        })
    }
}

/// Parse an index file. Strict: an unknown header or a malformed line refuses.
fn parse_index(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut lines = text.lines();
    match lines.next() {
        Some(header) if header == INDEX_HEADER => {}
        Some(other) => return Err(format!("unknown cache format {other:?}")),
        None => return Err("the cache index is empty".to_string()),
    }
    let mut index = BTreeMap::new();
    for (number, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let Some((digest, uri)) = line.split_once("  ") else {
            return Err(format!("line {} is not `<digest>  <uri>`", number + 2));
        };
        if digest.len() != 64
            || !digest
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        {
            return Err(format!(
                "line {} does not hold a lowercase hex sha256 digest",
                number + 2
            ));
        }
        if uri.is_empty() {
            return Err(format!("line {} names no source", number + 2));
        }
        index.insert(uri.to_string(), digest.to_string());
    }
    Ok(index)
}
