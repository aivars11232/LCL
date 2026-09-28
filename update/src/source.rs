//! Where updates come from: the latest stable GitHub Release of the official
//! repository, and nothing else.
//!
//! The repository is pinned in the build. A release is asked for through the
//! GitHub API's `releases/latest`, which leaves out drafts and pre-releases,
//! and a release that says it is either is still ignored. Assets are fetched
//! by name from that same release, at a URL built here, never from a URL a
//! manifest or an answer supplies. Only a test build may name a local test
//! server instead (`LCL_UPDATE_TEST_ENDPOINT`).

use crate::http::{self, Failure};
use lcl_spec::json::Json;

/// The official repository, `owner/name`.
pub const OFFICIAL_REPOSITORY: &str = "aivars11232/LCL";

/// The signed manifest and its detached signature, as release assets.
pub const MANIFEST_ASSET: &str = "update-manifest.json";
pub const SIGNATURE_ASSET: &str = "update-manifest.sig";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    api: String,
    download: String,
    allow_http: bool,
}

/// One published, stable release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

fn safe_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 200
        && !text.starts_with('.')
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

impl Source {
    /// The official GitHub Releases of [`OFFICIAL_REPOSITORY`].
    pub fn official() -> Source {
        Source {
            api: format!("https://api.github.com/repos/{OFFICIAL_REPOSITORY}"),
            download: format!("https://github.com/{OFFICIAL_REPOSITORY}/releases/download"),
            allow_http: false,
        }
    }

    /// A local test server at `base` (`http://127.0.0.1:<port>`), answering
    /// `/api/releases/latest` and `/download/<tag>/<name>` as GitHub would.
    pub fn test_server(base: &str) -> Result<Source, String> {
        let local = base
            .strip_prefix("http://127.0.0.1:")
            .or_else(|| base.strip_prefix("http://localhost:"))
            .is_some_and(|port| port.parse::<u16>().is_ok());
        if !local {
            return Err(format!(
                "{base:?} is not a local test server (http://127.0.0.1:<port>)"
            ));
        }
        Ok(Source {
            api: format!("{base}/api"),
            download: format!("{base}/download"),
            allow_http: true,
        })
    }

    /// The source this build uses: the official one, unless this is a test
    /// build and `LCL_UPDATE_TEST_ENDPOINT` names a local test server.
    pub fn configured() -> Result<Source, String> {
        #[cfg(feature = "test-endpoint")]
        if let Ok(base) = std::env::var("LCL_UPDATE_TEST_ENDPOINT") {
            return Source::test_server(&base);
        }
        Ok(Source::official())
    }

    /// What the source is, for `lcl-update source` and the release builder.
    pub fn describe(&self) -> String {
        if self.allow_http {
            format!("TEST SERVER {}", self.api.trim_end_matches("/api"))
        } else {
            format!("github.com/{OFFICIAL_REPOSITORY} releases (pinned)")
        }
    }

    /// The latest stable release, or `None` when there is none.
    pub fn latest(&self) -> Result<Option<Release>, Failure> {
        let url = format!("{}/releases/latest", self.api);
        let body = match http::get(
            &url,
            "application/vnd.github+json",
            1 << 20,
            self.allow_http,
            &mut |_, _| {},
        ) {
            Ok(body) => body,
            // No release at all: GitHub answers 404.
            Err(Failure::Status(404)) => return Ok(None),
            Err(other) => return Err(other),
        };
        let invalid = |why: &str| Failure::Invalid(format!("the release listing: {why}"));
        let text = String::from_utf8(body).map_err(|_| invalid("not UTF-8"))?;
        let json = lcl_spec::json::parse(&text).map_err(|e| invalid(&e.to_string()))?;
        let flag = |key: &str| json.get(key).and_then(Json::as_bool);
        let (Some(draft), Some(prerelease)) = (flag("draft"), flag("prerelease")) else {
            return Err(invalid("draft and prerelease must be true or false"));
        };
        if draft || prerelease {
            return Ok(None);
        }
        let tag = json
            .get("tag_name")
            .and_then(Json::as_str)
            .filter(|tag| safe_name(tag))
            .ok_or_else(|| invalid("no usable tag_name"))?
            .to_string();
        let mut assets = Vec::new();
        for asset in json
            .get("assets")
            .and_then(Json::as_array)
            .ok_or_else(|| invalid("no assets"))?
        {
            let name = asset.get("name").and_then(Json::as_str);
            let size = asset.get("size").and_then(Json::as_u64);
            let (Some(name), Some(size)) = (name, size) else {
                return Err(invalid("an asset without a name and size"));
            };
            if safe_name(name) {
                assets.push(Asset {
                    name: name.to_string(),
                    size,
                });
            }
        }
        Ok(Some(Release { tag, assets }))
    }

    /// Asset `name` of `release`, at most `limit` bytes.
    pub fn fetch(
        &self,
        release: &Release,
        name: &str,
        limit: u64,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>, Failure> {
        if release.asset(name).is_none() || !safe_name(&release.tag) {
            return Err(Failure::Invalid(format!(
                "release {} has no asset {name}",
                release.tag
            )));
        }
        let url = format!("{}/{}/{name}", self.download, release.tag);
        http::get(
            &url,
            "application/octet-stream",
            limit,
            self.allow_http,
            progress,
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::http::tests::{ok, serve};

    pub fn listing(tag: &str, draft: bool, prerelease: bool, assets: &[(&str, u64)]) -> Vec<u8> {
        let assets: Vec<String> = assets
            .iter()
            .map(|(name, size)| format!(r#"{{"name": "{name}", "size": {size}, "browser_download_url": "https://elsewhere.example/{name}"}}"#))
            .collect();
        ok(format!(
            r#"{{"tag_name": "{tag}", "draft": {draft}, "prerelease": {prerelease}, "published_at": "2026-10-01T12:00:00Z", "assets": [{}]}}"#,
            assets.join(", ")
        )
        .as_bytes())
    }

    #[test]
    fn only_a_published_stable_release_is_seen() {
        for (draft, prerelease) in [(true, false), (false, true)] {
            let server = serve(vec![(
                "/api/releases/latest".into(),
                listing("v9.0.0", draft, prerelease, &[]),
            )]);
            assert_eq!(
                Source::test_server(&server.base).unwrap().latest().unwrap(),
                None
            );
        }
        let none = serve(vec![]);
        assert_eq!(
            Source::test_server(&none.base).unwrap().latest().unwrap(),
            None
        );
        let server = serve(vec![
            (
                "/api/releases/latest".into(),
                listing("v0.2.0", false, false, &[("update-manifest.json", 4)]),
            ),
            ("/download/v0.2.0/update-manifest.json".into(), ok(b"sign")),
        ]);
        let source = Source::test_server(&server.base).unwrap();
        let release = source.latest().unwrap().unwrap();
        assert_eq!(release.tag, "v0.2.0");
        assert_eq!(
            source
                .fetch(&release, "update-manifest.json", 10, &mut |_, _| {})
                .unwrap(),
            b"sign"
        );
        // Only an asset the release lists, from the release itself.
        assert!(source
            .fetch(&release, "other.bin", 10, &mut |_, _| {})
            .is_err());
    }

    #[test]
    fn the_official_source_is_pinned_and_a_test_server_must_be_local() {
        assert_eq!(
            Source::official().describe(),
            "github.com/aivars11232/LCL releases (pinned)"
        );
        for base in [
            "https://evil.example",
            "http://10.0.0.1:80",
            "http://127.0.0.1",
            "http://127.0.0.1:x",
        ] {
            assert!(Source::test_server(base).is_err(), "{base}");
        }
        #[cfg(not(feature = "test-endpoint"))]
        {
            std::env::set_var("LCL_UPDATE_TEST_ENDPOINT", "http://127.0.0.1:1");
            assert_eq!(Source::configured().unwrap(), Source::official());
            std::env::remove_var("LCL_UPDATE_TEST_ENDPOINT");
        }
    }
}
