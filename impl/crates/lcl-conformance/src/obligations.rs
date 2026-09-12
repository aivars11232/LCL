//! Reviewed executable evidence requirements for exactly the approved Core.
//!
//! The embedded mapping is an implementation obligation, not an execution.
//! Its digest pins sub-probe membership independently of report callers and
//! case generators. Canonical registry membership is checked separately.

use crate::{report::ClaimLevel, ConformanceIndex};
use lcl_spec::{
    json::{self, Json},
    SpecPackage,
};
use std::collections::{BTreeMap, BTreeSet};

const MAPPING: &str = include_str!("obligations_v0.1.0.json");
pub const MAPPING_DIGEST: &str = "17fb6df8dbe055ae12be61e341fd3ca6ac637ee94f4ba218b9cc55a18164fbed";

#[derive(Debug, Clone)]
pub struct Obligation {
    pub id: String,
    pub level: ClaimLevel,
    pub authority: String,
    pub probes: BTreeSet<String>,
}

/// Only `load` constructs an inventory. No caller-supplied shortened list can
/// become an authoritative inventory for this version.
#[derive(Debug, Clone)]
pub struct Obligations {
    rows: BTreeMap<String, Obligation>,
    probes: BTreeMap<String, ClaimLevel>,
}

impl Obligations {
    pub fn load(spec: &SpecPackage) -> Result<Self, String> {
        if !spec.is_authoritative()
            || spec.formal_version() != "0.1.0"
            || spec.identity_digest() != lcl_spec::APPROVED_PACKAGE.identity_digest
        {
            return Err("obligations require the verified approved Core 0.1.0 package".into());
        }
        if lcl_spec::sha256::hex_digest(MAPPING.as_bytes()) != MAPPING_DIGEST {
            return Err("the reviewed obligation mapping digest does not match".into());
        }
        let mapping = json::parse(MAPPING).map_err(|error| error.to_string())?;
        if string(&mapping, "version")? != spec.formal_version()
            || string(&mapping, "package_identity")? != spec.identity_digest()
        {
            return Err("obligation mapping version or package identity mismatch".into());
        }
        let mut rows = BTreeMap::new();
        let mut probes = BTreeMap::new();
        for row in mapping
            .get("obligations")
            .and_then(Json::as_array)
            .ok_or("missing obligation rows")?
        {
            let id = string(row, "id")?.to_string();
            let level = match string(row, "level")? {
                "source" => ClaimLevel::Source,
                "semantics" => ClaimLevel::Semantics,
                _ => return Err(format!("invalid level for {id}")),
            };
            let mut required = BTreeSet::new();
            for value in row
                .get("probes")
                .and_then(Json::as_array)
                .ok_or("missing probes")?
            {
                let probe = value
                    .as_str()
                    .ok_or("probe ID is not a string")?
                    .to_string();
                if probe.is_empty()
                    || !required.insert(probe.clone())
                    || probes.insert(probe.clone(), level).is_some()
                {
                    return Err(format!("empty or duplicate required probe {probe}"));
                }
            }
            if required.is_empty() {
                return Err(format!("empty obligation {id}"));
            }
            let obligation = Obligation {
                id: id.clone(),
                level,
                authority: string(row, "authority")?.to_string(),
                probes: required,
            };
            if rows.insert(id.clone(), obligation).is_some() {
                return Err(format!("duplicate obligation {id}"));
            }
        }
        let inventory = Self { rows, probes };
        inventory.check_membership(spec)?;
        Ok(inventory)
    }

    fn check_membership(&self, spec: &SpecPackage) -> Result<(), String> {
        let fields = spec
            .registry("field_signatures")
            .and_then(|r| r.get("blocks"))
            .and_then(Json::as_object)
            .ok_or("field signatures absent")?;
        let mut block_ids = BTreeSet::new();
        let mut field_ids = BTreeSet::new();
        for (block, signature) in fields {
            block_ids.insert(format!("source/block/{block}"));
            for (field, _) in signature
                .get("fields")
                .and_then(Json::as_object)
                .ok_or("fields absent")?
            {
                field_ids.insert(format!("source/field/{block}/{field}"));
            }
        }
        self.exact_family("source/block/", block_ids)?;
        self.exact_family("source/field/", field_ids)?;
        let keywords = spec
            .registry("keywords")
            .and_then(|r| r.get("keywords"))
            .and_then(Json::as_object)
            .ok_or("keywords absent")?;
        self.exact_family(
            "source/keyword/",
            keywords
                .iter()
                .map(|(word, _)| format!("source/keyword/{word}"))
                .collect(),
        )?;
        let index = ConformanceIndex::load(spec).map_err(|e| e.to_string())?;
        self.exact_family(
            "CLOSURE-",
            index.witnesses().iter().map(|w| w.id.clone()).collect(),
        )?;
        // The complete additional contract inventory is tied to the canonical
        // catalogue subjects, not to whichever fixtures happen to exist.
        for category in [
            "type_valid",
            "type_invalid",
            "operator_valid",
            "operator_invalid",
            "function_valid",
            "function_invalid",
            "operation_binding",
            "operation_effects",
            "operation_errors",
            "status_transition",
            "error_contract",
            "result_schemas",
            "diagnostic_policy",
            "failure_lifecycle",
        ] {
            let prefix = format!("semantic/{category}/");
            self.exact_family(
                &prefix,
                index
                    .by_category(category)
                    .iter()
                    .map(|r| format!("{prefix}{}", r.subject))
                    .collect(),
            )?;
        }
        Ok(())
    }

    fn exact_family(&self, prefix: &str, expected: BTreeSet<String>) -> Result<(), String> {
        let actual = self
            .rows
            .keys()
            .filter(|id| id.starts_with(prefix))
            .cloned()
            .collect();
        if expected != actual {
            return Err(format!("incomplete or extraneous {prefix} obligations"));
        }
        Ok(())
    }

    pub fn rows(&self) -> impl Iterator<Item = &Obligation> {
        self.rows.values()
    }
    pub fn row(&self, id: &str) -> Option<&Obligation> {
        self.rows.get(id)
    }
    pub fn probes(&self) -> impl Iterator<Item = (&str, ClaimLevel)> {
        self.probes.iter().map(|(id, level)| (id.as_str(), *level))
    }
}

fn string<'a>(value: &'a Json, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing nonempty {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_bytes_match_the_reviewed_digest() {
        assert_eq!(
            lcl_spec::sha256::hex_digest(MAPPING.as_bytes()),
            MAPPING_DIGEST
        );
    }
}
