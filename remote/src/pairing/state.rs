//! `pairing.json`: the challenges and candidates as the PC keeps them, read,
//! written and changed under the pairing lock.

use super::{Candidate, Challenge, Pairing, Status};
use crate::paths;
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;

/// How long spent and expired challenges are kept, for the record.
const HISTORY_SECONDS: u64 = 86_400;

/// Everything in `pairing.json`.
#[derive(Default)]
pub(super) struct State {
    pub(super) challenges: Vec<Challenge>,
    pub(super) candidates: Vec<Candidate>,
}

impl State {
    /// Drop what no longer matters: challenges a day past their expiry, and
    /// candidates of expired codes that never became trusted. A candidate whose
    /// finalization began is kept with its challenge, so it can finish.
    pub(super) fn prune(&mut self, now: u64) {
        self.challenges
            .retain(|c| c.expires.saturating_add(HISTORY_SECONDS) > now);
        let challenges = &self.challenges;
        self.candidates.retain(|c| {
            challenges.iter().any(|ch| ch.id == c.challenge)
                && (now < c.expires || c.status == Status::Finalized || c.device.is_some())
        });
    }
}

impl Pairing {
    /// Read the state. A file that cannot be read is an error, never an empty
    /// state: pairing then stops rather than forgetting a decision.
    pub(super) fn load(&self) -> Result<State, String> {
        let text = match std::fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(e.to_string()),
        };
        let json = lcl_spec::json::parse(&text).map_err(|e| e.to_string())?;
        // Version 1 was written before approval existed and has no candidates.
        let version = json.get("version").and_then(Json::as_u64);
        if !matches!(version, Some(1) | Some(2)) {
            return Err("pairing.json is not a version this PC reads".into());
        }
        let challenges = json
            .get("challenges")
            .and_then(Json::as_array)
            .ok_or("pairing.json has no challenge list")?
            .iter()
            .map(|c| read_challenge(c).ok_or("pairing.json holds a malformed challenge"))
            .collect::<Result<Vec<_>, _>>()?;
        let candidates = match (version, json.get("candidates")) {
            (Some(1), None) => Vec::new(),
            (_, Some(list)) => list
                .as_array()
                .ok_or("pairing.json has a malformed candidate list")?
                .iter()
                .map(|c| read_candidate(c).ok_or("pairing.json holds a malformed candidate"))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err("pairing.json has no candidate list".into()),
        };
        Ok(State {
            challenges,
            candidates,
        })
    }

    pub(super) fn store(&self, state: &State) -> Result<(), String> {
        let challenges = state.challenges.iter().map(challenge_node);
        let candidates = state.candidates.iter().map(candidate_node);
        let text = Object::new()
            .with("version", Node::u64(2))
            .with("challenges", Node::array(challenges))
            .with("candidates", Node::array(candidates))
            .pretty();
        paths::write_private(&self.file, text.as_bytes()).map_err(|e| e.to_string())
    }

    /// Change the state under the pairing lock.
    pub(super) fn update<T, E: From<String>>(
        &self,
        now: u64,
        change: impl FnOnce(&mut State) -> Result<T, E>,
    ) -> Result<T, E> {
        let _held = paths::lock(&self.lock).map_err(|e| E::from(e.to_string()))?;
        let mut state = self.load().map_err(E::from)?;
        let out = change(&mut state)?;
        state.prune(now);
        self.store(&state).map_err(E::from)?;
        Ok(out)
    }
}

/// A challenge from `pairing.json`, or `None` if it lacks a field it must have.
fn read_challenge(json: &Json) -> Option<Challenge> {
    Some(Challenge {
        id: string(json, "id")?,
        hash: string(json, "hash")?,
        created: number(json, "created")?,
        expires: number(json, "expires")?,
        consumed_at: number(json, "consumed_at"),
        consumed_by: string(json, "consumed_by"),
        approved: string(json, "approved"),
    })
}

/// A candidate from `pairing.json`, or `None` if it lacks a field it must have.
fn read_candidate(json: &Json) -> Option<Candidate> {
    Some(Candidate {
        request: string(json, "request")?,
        challenge: string(json, "challenge")?,
        fingerprint: string(json, "fingerprint")?,
        name: string(json, "name")?,
        verification: string(json, "verification")?,
        created: number(json, "created")?,
        expires: number(json, "expires")?,
        status: Status::parse(&string(json, "status")?)?,
        decided_at: number(json, "decided_at"),
        device: string(json, "device"),
    })
}

fn string(json: &Json, key: &str) -> Option<String> {
    json.get(key).and_then(Json::as_str).map(str::to_string)
}

fn number(json: &Json, key: &str) -> Option<u64> {
    json.get(key).and_then(Json::as_u64)
}

/// A challenge as `pairing.json` holds it.
fn challenge_node(c: &Challenge) -> Node {
    Object::new()
        .with("id", Node::string(&c.id))
        .with("hash", Node::string(&c.hash))
        .with("created", Node::u64(c.created))
        .with("expires", Node::u64(c.expires))
        .with(
            "consumed_at",
            c.consumed_at.map(Node::u64).unwrap_or(Node::Null),
        )
        .with("consumed_by", Node::optional(c.consumed_by.clone()))
        .with("approved", Node::optional(c.approved.clone()))
        .into()
}

/// A candidate as `pairing.json` holds it.
fn candidate_node(c: &Candidate) -> Node {
    Object::new()
        .with("request", Node::string(&c.request))
        .with("challenge", Node::string(&c.challenge))
        .with("fingerprint", Node::string(&c.fingerprint))
        .with("name", Node::string(&c.name))
        .with("verification", Node::string(&c.verification))
        .with("created", Node::u64(c.created))
        .with("expires", Node::u64(c.expires))
        .with("status", Node::string(c.status.as_str()))
        .with(
            "decided_at",
            c.decided_at.map(Node::u64).unwrap_or(Node::Null),
        )
        .with("device", Node::optional(c.device.clone()))
        .into()
}
