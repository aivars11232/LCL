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

const MAPPING: &str = include_str!("obligations_v0.1.0_r2.json");
pub const MAPPING_DIGEST: &str = "386c14994f032a57143ef731ea7ac55db3dfffde52b54f0d38e39e7b5e126b20";

#[derive(Debug, Clone)]
pub struct Obligation {
    pub id: String,
    pub level: ClaimLevel,
    pub authority: String,
    pub probes: BTreeSet<String>,
    /// The exact sub-run labels this row's single grouped probe must carry,
    /// when the reviewed mapping pins them. A group whose recorded sub-runs
    /// differ from these, by omission, addition or duplication, does not
    /// establish the probe.
    pub subruns: Option<BTreeSet<String>>,
}

/// Only `load` constructs an inventory. No caller-supplied shortened list can
/// become an authoritative inventory for this version.
#[derive(Debug, Clone)]
pub struct Obligations {
    rows: BTreeMap<String, Obligation>,
    probes: BTreeMap<String, ClaimLevel>,
    subruns: BTreeMap<String, BTreeSet<String>>,
    /// The SHA-256 digest of the exact mapping text this inventory was loaded from.
    digest: String,
}

impl Obligations {
    pub fn load(spec: &SpecPackage) -> Result<Self, String> {
        Self::load_mapping(spec, MAPPING, MAPPING_DIGEST)
    }

    /// Load one mapping text under the digest it must match.
    ///
    /// Private: the only production caller is `load`, with the embedded
    /// reviewed mapping and its pinned digest.
    fn load_mapping(spec: &SpecPackage, mapping: &str, digest: &str) -> Result<Self, String> {
        if !spec.is_authoritative()
            || spec.formal_version() != "0.1.0"
            || spec.identity_digest() != lcl_spec::APPROVED_PACKAGE.identity_digest
        {
            return Err("obligations require the verified approved Core 0.1.0 package".into());
        }
        if lcl_spec::sha256::hex_digest(mapping.as_bytes()) != digest {
            return Err("the reviewed obligation mapping digest does not match".into());
        }
        let mapping = json::parse(mapping).map_err(|error| error.to_string())?;
        if string(&mapping, "version")? != spec.formal_version()
            || string(&mapping, "package_identity")? != spec.identity_digest()
        {
            return Err("obligation mapping version or package identity mismatch".into());
        }
        let mut rows = BTreeMap::new();
        let mut probes = BTreeMap::new();
        let mut subrun_pins = BTreeMap::new();
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
            let subruns = match row.get("subruns") {
                None => None,
                Some(value) => {
                    let labels = value
                        .as_array()
                        .ok_or_else(|| format!("subruns of {id} is not a list"))?;
                    let [probe] = required.iter().collect::<Vec<_>>()[..] else {
                        return Err(format!("subruns of {id} require exactly one grouped probe"));
                    };
                    let mut pinned = BTreeSet::new();
                    for label in labels {
                        let label = label
                            .as_str()
                            .filter(|label| !label.is_empty())
                            .ok_or_else(|| format!("empty or non-string sub-run label in {id}"))?;
                        if !pinned.insert(label.to_string()) {
                            return Err(format!("duplicate sub-run label {label} in {id}"));
                        }
                    }
                    if pinned.is_empty() {
                        return Err(format!("empty subruns in {id}"));
                    }
                    subrun_pins.insert(probe.clone(), pinned.clone());
                    Some(pinned)
                }
            };
            let obligation = Obligation {
                id: id.clone(),
                level,
                authority: string(row, "authority")?.to_string(),
                probes: required,
                subruns,
            };
            if rows.insert(id.clone(), obligation).is_some() {
                return Err(format!("duplicate obligation {id}"));
            }
        }
        let inventory = Self {
            rows,
            probes,
            subruns: subrun_pins,
            digest: digest.to_string(),
        };
        inventory.check_membership(spec)?;
        inventory.check_subrun_derivations(spec)?;
        Ok(inventory)
    }

    /// Test seam: load an altered mapping under its own digest, so a test can
    /// show that structure and membership checks still refuse it.
    #[cfg(test)]
    pub(crate) fn load_mapping_text(spec: &SpecPackage, mapping: &str) -> Result<Self, String> {
        Self::load_mapping(
            spec,
            mapping,
            &lcl_spec::sha256::hex_digest(mapping.as_bytes()),
        )
    }

    /// Test seam: mapping `text` with one row's sub-run pins set to the JSON
    /// value `pin`, replacing the row's pins when it already carries some.
    #[cfg(test)]
    pub(crate) fn with_row_subruns(text: &str, row: &str, pin: &str) -> String {
        let start = text
            .find(&format!("\"id\": \"{row}\""))
            .expect("the row is in the mapping text");
        let end = start + text[start..].find("\n    }").expect("the row closes");
        match text[start..end].find("\"subruns\": ") {
            Some(offset) => {
                let value = start + offset + "\"subruns\": ".len();
                let close = value + json_value_length(&text[value..end]);
                format!("{}{pin}{}", &text[..value], &text[close..])
            }
            None => format!(
                "{},\n      \"subruns\": {pin}{}",
                &text[..end],
                &text[end..]
            ),
        }
    }

    /// Test seam: the embedded reviewed mapping with one row pinned to `labels`.
    #[cfg(test)]
    pub(crate) fn embedded_mapping_with_subruns(row: &str, labels: &[&str]) -> String {
        let pins: Vec<String> = labels.iter().map(|label| format!("{label:?}")).collect();
        Self::with_row_subruns(MAPPING, row, &format!("[{}]", pins.join(", ")))
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

    /// Canonical cross-checks for the registry-derivable part of every pin set.
    ///
    /// A mapping revision may add requirement clauses. It cannot leave a
    /// semantic contract row unpinned, drop a sub-run the registries determine,
    /// pin an error the operation does not register, or index a condition the
    /// operation does not declare.
    fn check_subrun_derivations(&self, spec: &SpecPackage) -> Result<(), String> {
        if let Some(row) = self
            .rows
            .values()
            .find(|row| row.id.starts_with("semantic/") && row.subruns.is_none())
        {
            return Err(format!("semantic contract row {} pins no sub-runs", row.id));
        }
        let pins = |id: &str| self.rows.get(id).and_then(|row| row.subruns.as_ref());
        let require = |id: &str, labels: &[String]| -> Result<(), String> {
            let pinned = pins(id).ok_or_else(|| format!("{id} pins no sub-runs"))?;
            match labels.iter().find(|label| !pinned.contains(*label)) {
                Some(label) => Err(format!("{id} drops the registry-derived sub-run {label}")),
                None => Ok(()),
            }
        };
        let registry = |name: &str| {
            spec.registry(name)
                .ok_or_else(|| format!("{name} registry absent"))
        };
        let strings = |value: Option<&Json>| -> Vec<String> {
            value
                .and_then(Json::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Json::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        };

        let statuses_and_errors = registry("statuses_and_errors")?;
        let statuses = statuses_and_errors
            .get("statuses")
            .and_then(Json::as_object)
            .ok_or("statuses absent")?;
        for (from, _) in statuses {
            let id = format!("semantic/status_transition/{from}");
            let mut expected = BTreeSet::new();
            for to in statuses
                .iter()
                .map(|(to, _)| to.as_str())
                .chain(["status.invented"])
            {
                for root in [false, true] {
                    if !root || from != "status.skipped" {
                        expected.insert(format!("{to}/root={root}"));
                    }
                }
            }
            if pins(&id) != Some(&expected) {
                return Err(format!(
                    "{id} pins differ from the registered successor set"
                ));
            }
        }
        for (error, meta) in statuses_and_errors
            .get("errors")
            .and_then(Json::as_object)
            .ok_or("errors absent")?
        {
            let stage = meta.get("stage").and_then(Json::as_str).unwrap_or_default();
            let owner =
                stage_owner(stage).ok_or_else(|| format!("{error} has unknown stage {stage}"))?;
            require(
                &format!("semantic/error_contract/{error}"),
                &["registry-contract".to_string(), owner.to_string()],
            )?;
        }

        let operations = registry("operations")?;
        let axis = operations
            .get("axis_contract")
            .ok_or("axis contract absent")?;
        let concrete = |kind: &str| -> Result<Vec<String>, String> {
            let exclusive = axis
                .get(&format!("exclusive_{kind}"))
                .and_then(Json::as_str)
                .unwrap_or_default();
            Ok(axis
                .get(&format!("{kind}_definitions"))
                .and_then(Json::as_object)
                .ok_or_else(|| format!("{kind} definitions absent"))?
                .iter()
                .map(|(name, _)| name.clone())
                .filter(|name| name != exclusive)
                .collect())
        };
        let effect_names = concrete("effect")?;
        let dependency_names = concrete("dependency")?;
        let roles = axis
            .get("implementation_profile")
            .and_then(|profile| profile.get("required_roles_by_operation"))
            .and_then(Json::as_object)
            .ok_or("required roles absent")?;
        for (operation, contract) in operations
            .get("contracts")
            .and_then(Json::as_object)
            .ok_or("operation contracts absent")?
        {
            let parameters = contract
                .get("parameters")
                .and_then(Json::as_object)
                .ok_or_else(|| format!("{operation} parameters absent"))?;
            let preconditions = strings(contract.get("preconditions"));
            let postconditions = strings(contract.get("postconditions"));

            let mut errors = vec![
                "unregistered-parameter".to_string(),
                "positional-earliest-stage".to_string(),
            ];
            if !parameters.is_empty() {
                errors.push("duplicate-parameter".into());
            }
            if let Some(target) = contract.get("target") {
                errors.push("unresolved-target".into());
                if target.get("required").and_then(Json::as_bool) == Some(true) {
                    errors.push("required-target".into());
                }
            }
            let mut binding = vec!["exact-binding".to_string()];
            for (name, parameter) in parameters {
                if parameter.get("required").and_then(Json::as_bool) == Some(true) {
                    errors.push(format!("required-parameter/{name}"));
                }
                for constraint in strings(parameter.get("constraints")) {
                    if let Some((low, high)) = constraint.split_once("..") {
                        if let (Ok(low), Ok(high)) = (low.parse::<i64>(), high.parse::<i64>()) {
                            errors.push(format!("bound/{name}/{}", low - 1));
                            errors.push(format!("bound/{name}/{}", high + 1));
                        }
                    }
                }
                binding.extend(
                    SUPPLIED
                        .iter()
                        .map(|supplied| format!("default/{name}/{supplied}")),
                );
                if !matches!(parameter.get("default"), None | Some(Json::Null)) {
                    binding.push(format!("binding/default/{name}"));
                }
            }
            if preconditions
                .iter()
                .any(|pre| pre.contains("core.memory_write"))
            {
                errors.push("forbidden-memory-target".into());
                errors.push("forbidden-state-target".into());
            }
            let errors_row = format!("semantic/operation_errors/{operation}");
            require(&errors_row, &errors)?;
            let registered = strings(contract.get("errors"));
            if let Some(label) = pins(&errors_row).into_iter().flatten().find(|label| {
                label
                    .strip_prefix("error/")
                    .is_some_and(|suffix| !registered.contains(&format!("error.{suffix}")))
            }) {
                return Err(format!(
                    "{errors_row} pins {label}, an error the operation does not register"
                ));
            }
            require(&format!("semantic/operation_binding/{operation}"), &binding)?;

            let category = contract
                .get("determinism")
                .and_then(|determinism| determinism.get("category"))
                .and_then(Json::as_str)
                .unwrap_or_default();
            let mut effects = vec!["actual-post-state".to_string()];
            for selection in ["none", "fixed", "variable", "two-fixed", "mixed"] {
                for graph in ["None", "Some(Deterministic)", "Some(Nondeterministic)"] {
                    effects.push(format!("determinism/{selection}/{graph}"));
                }
            }
            let mut row_roles = BTreeSet::new();
            if let Some((_, modes)) = roles.iter().find(|(name, _)| name == operation) {
                for (_, list) in modes.as_object().unwrap_or_default() {
                    row_roles.extend(strings(Some(list)));
                }
            }
            let maximum_effects = strings(contract.get("possible_effects"));
            let maximum_dependencies = strings(contract.get("possible_dependencies"));
            for role in &row_roles {
                for scenario in ["missing", "complete", "ambiguous"] {
                    effects.push(format!("profile/{role}/{scenario}"));
                }
                for field in ["implementation", "version", "source", "resolution"] {
                    effects.push(format!("profile/{role}/incomplete/{field}"));
                }
                if category != "inherited" {
                    for effect in effect_names
                        .iter()
                        .filter(|name| !maximum_effects.contains(*name))
                    {
                        effects.push(format!("profile/{role}/forbidden-effect/{effect}"));
                    }
                    for dependency in dependency_names
                        .iter()
                        .filter(|name| !maximum_dependencies.contains(*name))
                    {
                        effects.push(format!("profile/{role}/forbidden-dependency/{dependency}"));
                    }
                }
                if category == "deterministic" {
                    effects.push(format!("profile/{role}/nondeterministic-under-fixed"));
                }
            }
            let effects_row = format!("semantic/operation_effects/{operation}");
            require(&effects_row, &effects)?;
            if let Some(label) = pins(&effects_row).into_iter().flatten().find(|label| {
                let beyond = |prefix: &str, count: usize| {
                    label
                        .strip_prefix(prefix)
                        .and_then(|index| index.parse::<usize>().ok())
                        .is_some_and(|index| index >= count)
                };
                beyond("precondition/", preconditions.len())
                    || beyond("postcondition/", postconditions.len())
            }) {
                return Err(format!(
                    "{effects_row} pins {label}, beyond the registered conditions"
                ));
            }
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
    /// The exact sub-run labels one required grouped probe must carry, if
    /// the reviewed mapping pins them.
    pub fn subruns(&self, probe: &str) -> Option<&BTreeSet<String>> {
        self.subruns.get(probe)
    }
    /// The SHA-256 digest of the exact mapping text this inventory was loaded from.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Rendered supplied values of the production parameter-default component runs.
const SUPPLIED: [&str; 6] = [
    "None",
    "Some(Missing)",
    "Some(Unknown)",
    "Some(Null)",
    "Some(Boolean(false))",
    "Some(Text(\"\"))",
];

/// The implementing component whose registry mirrors a registered stage.
fn stage_owner(stage: &str) -> Option<&'static str> {
    Some(match stage {
        "lexical" => "lexer",
        "grammar_or_schema" => "parser",
        "resolution" => "resolver",
        "static_or_expression" => "checker",
        "validation" => "preflight",
        "execution" => "runtime",
        "verification_or_completion" => "completion",
        _ => return None,
    })
}

fn string<'a>(value: &'a Json, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing nonempty {key}"))
}

/// The byte length of the JSON list or string value that begins `text`.
#[cfg(test)]
fn json_value_length(text: &str) -> usize {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (index, byte) in text.bytes().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
                if depth == 0 {
                    return index + 1;
                }
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b'[' {
            depth += 1;
        } else if byte == b']' {
            depth -= 1;
            if depth == 0 {
                return index + 1;
            }
        } else if depth == 0 && (byte == b',' || byte == b'\n') {
            return index;
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The last row of the mapping, in every revision.
    const ROW: &str = "semantic/failure_lifecycle/core.failure_lifecycle";
    const LAST_PROBES: &str =
        "\"probes\": [\n        \"semantic/failure_lifecycle/core.failure_lifecycle\"\n      ]";

    fn spec() -> SpecPackage {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../canonical/LCL_Core_0.1.0");
        SpecPackage::open(root).unwrap()
    }

    #[test]
    fn mapping_bytes_match_the_reviewed_digest() {
        assert_eq!(
            lcl_spec::sha256::hex_digest(MAPPING.as_bytes()),
            MAPPING_DIGEST
        );
    }

    #[test]
    fn a_mapping_under_another_digest_is_refused() {
        let error = Obligations::load_mapping(&spec(), MAPPING, &"0".repeat(64)).unwrap_err();
        assert!(error.contains("digest"), "{error}");
    }

    #[test]
    fn a_truncated_inventory_is_refused_even_under_its_own_digest() {
        let start = MAPPING
            .rfind(&format!(",\n    {{\n      \"id\": \"{ROW}\""))
            .expect("the last row is present");
        let end = start
            + MAPPING[start..]
                .find("\n    }")
                .expect("the last row closes")
            + 6;
        let truncated = format!("{}{}", &MAPPING[..start], &MAPPING[end..]);
        let error = Obligations::load_mapping_text(&spec(), &truncated).unwrap_err();
        assert!(
            error.contains("incomplete or extraneous semantic/failure_lifecycle/"),
            "{error}"
        );
        // Control: the complete text loads under its own digest.
        assert!(Obligations::load_mapping_text(&spec(), MAPPING).is_ok());
    }

    #[test]
    fn pinned_subruns_are_parsed_exactly_and_malformed_pins_are_refused() {
        let pinned = |pin: &str| Obligations::with_row_subruns(MAPPING, ROW, pin);
        let inventory = Obligations::load_mapping_text(&spec(), &pinned("[\"a\", \"b\"]")).unwrap();
        let expected: BTreeSet<String> = ["a", "b"].into_iter().map(String::from).collect();
        assert_eq!(inventory.subruns(ROW), Some(&expected));
        assert_eq!(
            inventory.row(ROW).and_then(|row| row.subruns.as_ref()),
            Some(&expected)
        );
        // Decision-witness probes are never pinned groups.
        assert_eq!(inventory.subruns("CLOSURE-059/exhausted"), None);
        for malformed in ["[]", "[\"a\", \"a\"]", "[\"\"]", "[1]", "\"a\""] {
            let error = Obligations::load_mapping_text(&spec(), &pinned(malformed)).unwrap_err();
            assert!(error.contains(ROW), "{malformed}: {error}");
        }
        // A pin on a row with several probes cannot say which probe it binds.
        let single = Obligations::embedded_mapping_with_subruns(ROW, &["a"]);
        let several = single.replacen(
            LAST_PROBES,
            "\"probes\": [\n        \"semantic/failure_lifecycle/core.failure_lifecycle\",\n        \"semantic/failure_lifecycle/core.failure_lifecycle/extra\"\n      ]",
            1,
        );
        assert_ne!(several, single, "the fixture probes block must be present");
        let error = Obligations::load_mapping_text(&spec(), &several).unwrap_err();
        assert!(error.contains("exactly one grouped probe"), "{error}");
    }

    #[test]
    fn the_pin_seam_inserts_and_replaces_one_rows_pins_only() {
        let once = Obligations::with_row_subruns(MAPPING, ROW, "[\"a\"]");
        let twice = Obligations::with_row_subruns(&once, ROW, "[\"b\", \"c\"]");
        let inventory = Obligations::load_mapping_text(&spec(), &twice).unwrap();
        let expected: BTreeSet<String> = ["b", "c"].into_iter().map(String::from).collect();
        assert_eq!(inventory.subruns(ROW), Some(&expected));
        let start = MAPPING.find(&format!("\"id\": \"{ROW}\"")).unwrap();
        let row_end = twice[start..].find("\n    }").unwrap();
        assert_eq!(
            twice[start..start + row_end].matches("\"subruns\"").count(),
            1
        );
        assert_eq!(&twice[..start], &MAPPING[..start]);
        assert_eq!(
            &twice[start + row_end..],
            &MAPPING[MAPPING.rfind("\n    }").unwrap()..]
        );
    }

    #[test]
    fn an_inventory_states_the_digest_of_the_mapping_it_loaded() {
        assert_eq!(Obligations::load(&spec()).unwrap().digest(), MAPPING_DIGEST);
        let pinned = Obligations::embedded_mapping_with_subruns(ROW, &["a"]);
        let inventory = Obligations::load_mapping_text(&spec(), &pinned).unwrap();
        assert_eq!(
            inventory.digest(),
            lcl_spec::sha256::hex_digest(pinned.as_bytes())
        );
        assert_ne!(inventory.digest(), MAPPING_DIGEST);
    }

    /// The embedded mapping with one pinned row's labels edited.
    fn repinned(row: &str, edit: impl FnOnce(&mut Vec<String>)) -> String {
        let inventory = Obligations::load(&spec()).unwrap();
        let mut labels: Vec<String> = inventory
            .subruns(row)
            .expect("the row is pinned")
            .iter()
            .cloned()
            .collect();
        edit(&mut labels);
        let pins: Vec<String> = labels.iter().map(|label| format!("{label:?}")).collect();
        Obligations::with_row_subruns(MAPPING, row, &format!("[{}]", pins.join(", ")))
    }

    /// The embedded mapping with one row's pins removed entirely.
    fn unpinned(row: &str) -> String {
        let start = MAPPING.find(&format!("\"id\": \"{row}\"")).unwrap();
        let end = start + MAPPING[start..].find("\n    }").unwrap();
        let key = start
            + MAPPING[start..end]
                .find(",\n      \"subruns\": ")
                .expect("the row is pinned");
        let value = key + ",\n      \"subruns\": ".len();
        let close = value + json_value_length(&MAPPING[value..end]);
        format!("{}{}", &MAPPING[..key], &MAPPING[close..])
    }

    #[test]
    fn registry_derived_pins_cannot_be_dropped_invented_or_left_out() {
        let cases = [
            (
                "semantic/status_transition/status.not_started",
                repinned("semantic/status_transition/status.not_started", |labels| {
                    labels.pop();
                }),
                "registered successor set",
            ),
            (
                "semantic/error_contract/error.cancelled",
                repinned("semantic/error_contract/error.cancelled", |labels| {
                    labels.retain(|label| label != "registry-contract")
                }),
                "drops the registry-derived sub-run registry-contract",
            ),
            (
                "semantic/operation_binding/core.inspect",
                repinned("semantic/operation_binding/core.inspect", |labels| {
                    labels.retain(|label| label != "binding/default/depth")
                }),
                "drops the registry-derived sub-run binding/default/depth",
            ),
            (
                "semantic/operation_errors/core.inspect",
                repinned("semantic/operation_errors/core.inspect", |labels| {
                    labels.push("error/invented.failure".into())
                }),
                "an error the operation does not register",
            ),
            (
                "semantic/operation_effects/core.rename",
                repinned("semantic/operation_effects/core.rename", |labels| {
                    labels.push("postcondition/99".into())
                }),
                "beyond the registered conditions",
            ),
            (
                "semantic/type_valid/STRING",
                unpinned("semantic/type_valid/STRING"),
                "pins no sub-runs",
            ),
        ];
        for (row, mapping, fragment) in cases {
            let error = Obligations::load_mapping_text(&spec(), &mapping).unwrap_err();
            assert!(
                error.contains(row) && error.contains(fragment),
                "{row}: {error}"
            );
        }
    }
}
