//! One-time pairing: the QR code a device scans, the challenge behind it, and
//! the requests devices make with it.
//!
//! Pairing establishes long-lived trust once; it is not how a device connects
//! afterwards. The QR code carries plain **pairing text**, not a link: no URI
//! scheme, nothing a phone's camera or browser treats as something to open.
//!
//! ```text
//! LCLPAIR|v=2&pc=<pc id>&n=<pc name>&fp=<certificate SHA-256>
//!         &a=<host:port>[&a=...]&c=<one-time code>&e=<expiry>
//! ```
//!
//! * `fp` lets the device authenticate the PC before saying anything to it:
//!   the TLS connection is refused unless the PC presents exactly that
//!   certificate.
//! * `a` is where to try reaching it. Addresses are hints for finding the PC,
//!   never part of who it is.
//! * `c` is a 32-byte random secret, good for a few minutes. The PC keeps only
//!   its SHA-256 and refuses it after its expiry.
//!
//! No private key is ever in the text.
//!
//! ## A code asks; the person at the PC decides
//!
//! Whoever holds the code — the person's own phone, or an app that read the
//! QR code over their shoulder — can only **ask** to be trusted. A device that
//! presents a live code with a certificate it holds the key for becomes a
//! pending [`Candidate`], bound to that challenge and to that certificate's
//! fingerprint, and nothing more: it is not in the device registry, it gets
//! no session, and the code is not used up, so a stranger who asks first
//! cannot lock the real phone out. The person at the PC compares a short
//! verification code the phone and the PC both show ([`verification`]) and
//! approves exactly one candidate. Only then, the next time that very
//! certificate asks with that very code, is the device trusted and the code
//! spent; every other candidate for the code is superseded.
//!
//! ## Finalization, and a crash in the middle of it
//!
//! Trust is written to `devices.json` and the pairing state to
//! `pairing.json`, two files. Finalization is ordered so that no crash can
//! leave a state from which any other certificate becomes trusted:
//!
//! 1. under the pairing lock, the approved candidate is given the id its
//!    device record will have, and that is written first;
//! 2. the device registry adds the record under that id, or finds it there
//!    already ([`crate::devices::Registry::enroll`]);
//! 3. the candidate is marked finalized, the code spent and the other
//!    candidates superseded, and that is written.
//!
//! A crash after 1 or 2 leaves an approved candidate with a reserved id,
//! which only the approved certificate can finish; approving another is
//! refused while one is approved. Locks are taken in one order only: the
//! pairing lock, then the device registry's.

use crate::b64;
use crate::devices::{clean_name, constant_time_eq, Device, Registry};
use crate::identity::{hex, random};
use crate::paths::{self, Paths};
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;
use ring::rand::SystemRandom;
use std::path::PathBuf;

/// What pairing text starts with.
pub const PREFIX: &str = "LCLPAIR|";
/// The pairing-text format this build writes and reads.
pub const PAYLOAD_VERSION: u32 = 2;
/// The pairing flow a device's `hello` must name (`"pairing_version": 2`):
/// the one in which the PC approves each new device.
pub const PAIRING_VERSION: u64 = 2;
/// How long a code is good for by default.
pub const DEFAULT_TTL_SECONDS: u64 = 300;
/// The most candidates waiting for a decision on one code at once.
pub const MAX_PENDING_PER_CHALLENGE: usize = 8;
/// The most candidates waiting for a decision on this PC at once.
pub const MAX_PENDING: usize = 32;
/// The most candidates one code ever records, decided ones included.
pub const MAX_CANDIDATES_PER_CHALLENGE: usize = 32;
/// How long spent and expired challenges are kept, for the record.
const HISTORY_SECONDS: u64 = 86_400;

/// The earlier pairing link, which trusted whoever used the code first.
const LEGACY_PREFIX: &str = "lclpair://";
/// Said when a person has pairing text or a device from before approval.
pub const OLDER_FLOW: &str =
    "This pairing code uses the older pairing flow. Update LCL on the PC and show a new QR code.";

/// One pairing challenge, as the PC stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub id: String,
    /// SHA-256 of the code, hex. The code itself is never stored.
    pub hash: String,
    pub created: u64,
    pub expires: u64,
    /// When a device finished pairing with it.
    pub consumed_at: Option<u64>,
    /// The fingerprint of the device that finished pairing with it.
    pub consumed_by: Option<String>,
    /// The one candidate the PC approved, by request id. Once set, no other
    /// candidate for this challenge can be approved or trusted.
    pub approved: Option<String>,
}

/// Where a candidate stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Waiting for the person at the PC.
    Pending,
    /// Approved on the PC; trusted once the same certificate asks again.
    Approved,
    /// Denied on the PC. It never becomes anything else.
    Denied,
    /// Trusted: its device record exists and the code is spent.
    Finalized,
    /// Another candidate for the same code was approved.
    Superseded,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Approved => "approved",
            Status::Denied => "denied",
            Status::Finalized => "finalized",
            Status::Superseded => "superseded",
        }
    }

    fn parse(text: &str) -> Option<Status> {
        Some(match text {
            "pending" => Status::Pending,
            "approved" => Status::Approved,
            "denied" => Status::Denied,
            "finalized" => Status::Finalized,
            "superseded" => Status::Superseded,
            _ => return None,
        })
    }
}

/// A device that asked to be trusted with a live code.
///
/// It is identified by the challenge and the fingerprint of the certificate it
/// proved it holds the key for. The request id only names it for the person
/// deciding; it proves nothing about the device, and the name is the device's
/// own claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub request: String,
    pub challenge: String,
    pub fingerprint: String,
    pub name: String,
    /// The short code the phone shows too, `abcd-ef12-3456`.
    pub verification: String,
    pub created: u64,
    /// The challenge's expiry: a candidate lives no longer than its code.
    pub expires: u64,
    pub status: Status,
    pub decided_at: Option<u64>,
    /// The id its device record gets, reserved when finalization begins.
    pub device: Option<String>,
}

impl Candidate {
    /// Its status as the person should read it now: an undecided or unused
    /// approval past its code's expiry is expired.
    pub fn status_at(&self, now: u64) -> &'static str {
        match self.status {
            Status::Pending | Status::Approved if now >= self.expires && self.device.is_none() => {
                "expired"
            }
            status => status.as_str(),
        }
    }

    /// Waiting for a decision, or approved and not yet trusted, and not expired.
    pub fn is_open(&self, now: u64) -> bool {
        matches!(self.status, Status::Pending | Status::Approved) && now < self.expires
    }
}

/// Why a device's request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No challenge has this code.
    Unknown,
    Expired,
    /// A device was already approved or paired with it.
    Used,
    /// The PC denied this device's request.
    Denied,
    /// Too many requests are waiting for a decision.
    Busy,
    Unreadable(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Unknown => f.write_str("this pairing code is not one this PC issued"),
            Refusal::Expired => f.write_str("this pairing code has expired; show a new QR code"),
            Refusal::Used => f.write_str("this pairing code was already used; show a new QR code"),
            Refusal::Denied => f.write_str("the PC denied this device's pairing request"),
            Refusal::Busy => f.write_str(
                "too many pairing requests are waiting on this PC; deny the ones you do not recognise, or show a new QR code",
            ),
            Refusal::Unreadable(detail) => write!(f, "pairing is unavailable: {detail}"),
        }
    }
}

/// What a device's request with a live code came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presented {
    /// Recorded, or already recorded, and waiting for the person at the PC.
    Pending(Candidate),
    /// Approved for exactly this certificate: the device is trusted now.
    Paired(Device),
}

/// The short verification code a phone and the PC both show for one
/// candidate, `abcd-ef12-3456`: the first 48 bits of
///
/// ```text
/// SHA-256("lcl-pair-v2" 0x00 pc_fingerprint 0x00 hex(SHA-256(code)) 0x00 candidate_fingerprint)
/// ```
///
/// It binds the PC, the code and the candidate's certificate, so the person
/// can tell their phone's request from anyone else's. It is not a secret, and
/// approval is bound to the full certificate fingerprint, not to this.
/// `code_hash` is what the PC stores for a challenge, so the code itself is
/// not needed to compute it.
pub fn verification(pc_fingerprint: &str, code_hash: &str, candidate_fingerprint: &str) -> String {
    let material = format!("lcl-pair-v2\0{pc_fingerprint}\0{code_hash}\0{candidate_fingerprint}");
    let digest = lcl_spec::sha256::hex_digest(material.as_bytes());
    format!("{}-{}-{}", &digest[0..4], &digest[4..8], &digest[8..12])
}

/// Everything in `pairing.json`.
#[derive(Default)]
struct State {
    challenges: Vec<Challenge>,
    candidates: Vec<Candidate>,
}

impl State {
    /// Drop what no longer matters: challenges a day past their expiry, and
    /// candidates of expired codes that never became trusted. A candidate whose
    /// finalization began is kept with its challenge, so it can finish.
    fn prune(&mut self, now: u64) {
        self.challenges
            .retain(|c| c.expires.saturating_add(HISTORY_SECONDS) > now);
        let challenges = &self.challenges;
        self.candidates.retain(|c| {
            challenges.iter().any(|ch| ch.id == c.challenge)
                && (now < c.expires || c.status == Status::Finalized || c.device.is_some())
        });
    }
}

/// The challenges and candidates, in the state directory.
#[derive(Debug, Clone)]
pub struct Pairing {
    file: PathBuf,
    lock: PathBuf,
}

impl Pairing {
    pub fn new(paths: &Paths) -> Pairing {
        Pairing {
            file: paths.state.join("pairing.json"),
            lock: paths.state.join("pairing.lock"),
        }
    }

    /// Read the state. A file that cannot be read is an error, never an empty
    /// state: pairing then stops rather than forgetting a decision.
    fn load(&self) -> Result<State, String> {
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
            .map(|c| {
                let text = |k: &str| c.get(k).and_then(Json::as_str).map(str::to_string);
                let number = |k: &str| c.get(k).and_then(Json::as_u64);
                (|| {
                    Some(Challenge {
                        id: text("id")?,
                        hash: text("hash")?,
                        created: number("created")?,
                        expires: number("expires")?,
                        consumed_at: number("consumed_at"),
                        consumed_by: text("consumed_by"),
                        approved: text("approved"),
                    })
                })()
                .ok_or_else(|| "pairing.json holds a malformed challenge".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let candidates = match (version, json.get("candidates")) {
            (Some(1), None) => Vec::new(),
            (_, Some(list)) => list
                .as_array()
                .ok_or("pairing.json has a malformed candidate list")?
                .iter()
                .map(|c| {
                    let text = |k: &str| c.get(k).and_then(Json::as_str).map(str::to_string);
                    let number = |k: &str| c.get(k).and_then(Json::as_u64);
                    (|| {
                        Some(Candidate {
                            request: text("request")?,
                            challenge: text("challenge")?,
                            fingerprint: text("fingerprint")?,
                            name: text("name")?,
                            verification: text("verification")?,
                            created: number("created")?,
                            expires: number("expires")?,
                            status: Status::parse(&text("status")?)?,
                            decided_at: number("decided_at"),
                            device: text("device"),
                        })
                    })()
                    .ok_or_else(|| "pairing.json holds a malformed candidate".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err("pairing.json has no candidate list".into()),
        };
        Ok(State {
            challenges,
            candidates,
        })
    }

    fn store(&self, state: &State) -> Result<(), String> {
        let challenges = state.challenges.iter().map(|c| {
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
        });
        let candidates = state.candidates.iter().map(|c| {
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
        });
        let text = Object::new()
            .with("version", Node::u64(2))
            .with("challenges", Node::array(challenges))
            .with("candidates", Node::array(candidates))
            .pretty();
        paths::write_private(&self.file, text.as_bytes()).map_err(|e| e.to_string())
    }

    /// Change the state under the pairing lock.
    fn update<T, E: From<String>>(
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

    /// Issue a code good for `ttl` seconds from `now`. Returns the challenge
    /// and the code, which exists nowhere else once this returns.
    pub fn create(&self, now: u64, ttl: u64) -> Result<(Challenge, String), String> {
        let rng = SystemRandom::new();
        let code = b64::encode(&random::<32>(&rng)?);
        let challenge = Challenge {
            id: hex(&random::<8>(&rng)?),
            hash: lcl_spec::sha256::hex_digest(code.as_bytes()),
            created: now,
            expires: now.saturating_add(ttl),
            consumed_at: None,
            consumed_by: None,
            approved: None,
        };
        self.update(now, |state| {
            state.challenges.push(challenge.clone());
            Ok::<_, String>(())
        })?;
        Ok((challenge, code))
    }

    /// A device presents `code` with the certificate whose fingerprint is
    /// `fingerprint`. The first time, it becomes a pending candidate; while
    /// the PC has not decided, it stays one; once the PC approved exactly this
    /// candidate, it is trusted here and now — a device record is made in
    /// `registry` and the code is spent.
    ///
    /// Nothing but approval on the PC makes trust: holding the code, asking
    /// first, asking often or asking with a request id never does.
    #[allow(clippy::too_many_arguments)]
    pub fn present(
        &self,
        code: &str,
        fingerprint: &str,
        name: &str,
        pc_fingerprint: &str,
        registry: &Registry,
        protocol: &str,
        now: u64,
    ) -> Result<Presented, Refusal> {
        let hash = lcl_spec::sha256::hex_digest(code.as_bytes());
        let _held = paths::lock(&self.lock).map_err(|e| Refusal::Unreadable(e.to_string()))?;
        let mut state = self.load().map_err(Refusal::Unreadable)?;
        let at = state
            .challenges
            .iter()
            .position(|c| constant_time_eq(c.hash.as_bytes(), hash.as_bytes()))
            .ok_or(Refusal::Unknown)?;
        let challenge = state.challenges[at].clone();
        let known = state.candidates.iter().position(|c| {
            c.challenge == challenge.id
                && constant_time_eq(c.fingerprint.as_bytes(), fingerprint.as_bytes())
        });

        let Some(index) = known else {
            // A new candidate. Nothing about it is trusted.
            if challenge.consumed_at.is_some() || challenge.approved.is_some() {
                return Err(Refusal::Used);
            }
            if now >= challenge.expires {
                return Err(Refusal::Expired);
            }
            let waiting = |c: &&Candidate| c.status == Status::Pending && now < c.expires;
            let here = || {
                state
                    .candidates
                    .iter()
                    .filter(|c| c.challenge == challenge.id)
            };
            if here().filter(waiting).count() >= MAX_PENDING_PER_CHALLENGE
                || here().count() >= MAX_CANDIDATES_PER_CHALLENGE
                || state.candidates.iter().filter(waiting).count() >= MAX_PENDING
            {
                return Err(Refusal::Busy);
            }
            let request = hex(&random::<8>(&SystemRandom::new()).map_err(Refusal::Unreadable)?);
            let candidate = Candidate {
                request,
                challenge: challenge.id.clone(),
                fingerprint: fingerprint.to_string(),
                name: clean_name(name),
                verification: verification(pc_fingerprint, &challenge.hash, fingerprint),
                created: now,
                expires: challenge.expires,
                status: Status::Pending,
                decided_at: None,
                device: None,
            };
            state.candidates.push(candidate.clone());
            state.prune(now);
            self.store(&state).map_err(Refusal::Unreadable)?;
            return Ok(Presented::Pending(candidate));
        };

        let candidate = state.candidates[index].clone();
        match candidate.status {
            Status::Denied => Err(Refusal::Denied),
            Status::Superseded => Err(Refusal::Used),
            // Asked again after it was trusted — the answer to the request that
            // finished pairing was lost. It is the same device only while that
            // very record is still trusted; a revoked one is not revived.
            Status::Finalized => match registry.authorize(fingerprint) {
                Ok(device) if Some(&device.id) == candidate.device.as_ref() => {
                    Ok(Presented::Paired(device))
                }
                Ok(_)
                | Err(crate::devices::Refusal::Unknown | crate::devices::Refusal::Revoked) => {
                    Err(Refusal::Used)
                }
                Err(crate::devices::Refusal::Unreadable(e)) => Err(Refusal::Unreadable(e)),
            },
            Status::Pending => {
                if now >= candidate.expires {
                    Err(Refusal::Expired)
                } else if challenge.consumed_at.is_some() || challenge.approved.is_some() {
                    Err(Refusal::Used)
                } else {
                    Ok(Presented::Pending(candidate))
                }
            }
            Status::Approved => {
                // Approved for this challenge and this certificate, and for no
                // other: the challenge names this very request.
                if challenge.approved.as_deref() != Some(candidate.request.as_str())
                    || challenge
                        .consumed_by
                        .as_deref()
                        .is_some_and(|by| !constant_time_eq(by.as_bytes(), fingerprint.as_bytes()))
                {
                    return Err(Refusal::Used);
                }
                // An approval not acted on before the code expired lapses. One
                // whose finalization already began is finished.
                if now >= candidate.expires && candidate.device.is_none() {
                    return Err(Refusal::Expired);
                }
                // 1. Reserve the device id, and write that first.
                let id = match candidate.device.clone() {
                    Some(id) => id,
                    None => {
                        let id =
                            hex(&random::<8>(&SystemRandom::new()).map_err(Refusal::Unreadable)?);
                        state.candidates[index].device = Some(id.clone());
                        self.store(&state).map_err(Refusal::Unreadable)?;
                        id
                    }
                };
                // 2. Trust exactly this certificate, under that id.
                let device = registry
                    .enroll(&id, &candidate.name, fingerprint, protocol, now)
                    .map_err(Refusal::Unreadable)?;
                // 3. Spend the code; nobody else can use it now.
                let entry = &mut state.candidates[index];
                entry.status = Status::Finalized;
                entry.decided_at = Some(now);
                let challenge = &mut state.challenges[at];
                challenge.consumed_at = Some(now);
                challenge.consumed_by = Some(fingerprint.to_string());
                let spent = challenge.id.clone();
                for other in state.candidates.iter_mut() {
                    if other.challenge == spent
                        && matches!(other.status, Status::Pending | Status::Approved)
                    {
                        other.status = Status::Superseded;
                        other.decided_at = Some(now);
                    }
                }
                state.prune(now);
                self.store(&state).map_err(Refusal::Unreadable)?;
                Ok(Presented::Paired(device))
            }
        }
    }

    /// Candidates waiting for a decision, or approved and not yet trusted,
    /// oldest first.
    pub fn candidates(&self, now: u64) -> Result<Vec<Candidate>, String> {
        let mut open: Vec<Candidate> = self
            .load()?
            .candidates
            .into_iter()
            .filter(|c| c.is_open(now))
            .collect();
        open.sort_by_key(|c| c.created);
        Ok(open)
    }

    /// Approve exactly one candidate, by its request id. Refused if it is not
    /// pending, if its code expired, or if another candidate for the same code
    /// was approved already: one code trusts at most one device.
    pub fn approve(&self, request: &str, now: u64) -> Result<Candidate, String> {
        self.update(now, |state| {
            let index = find(state, request)?;
            let candidate = state.candidates[index].clone();
            match candidate.status {
                Status::Pending => {}
                Status::Approved => {
                    return Err(format!("pairing request {request} is already approved"))
                }
                Status::Denied => return Err(format!("pairing request {request} was denied")),
                Status::Finalized => {
                    return Err(format!("pairing request {request} already paired its device"))
                }
                Status::Superseded => {
                    return Err(format!(
                        "another device was approved with the same pairing code; request {request} can no longer be approved"
                    ))
                }
            }
            if now >= candidate.expires {
                return Err(format!(
                    "pairing request {request} expired with its code; show a new QR code"
                ));
            }
            let challenge = state
                .challenges
                .iter_mut()
                .find(|c| c.id == candidate.challenge)
                .ok_or_else(|| format!("pairing request {request} has no pairing code any more"))?;
            if challenge.consumed_at.is_some() || challenge.approved.is_some() {
                return Err(format!(
                    "another device was approved with the same pairing code; request {request} can no longer be approved"
                ));
            }
            challenge.approved = Some(candidate.request.clone());
            for other in state.candidates.iter_mut() {
                if other.challenge == candidate.challenge {
                    if other.request == candidate.request {
                        other.status = Status::Approved;
                    } else if other.status == Status::Pending {
                        other.status = Status::Superseded;
                    } else {
                        continue;
                    }
                    other.decided_at = Some(now);
                }
            }
            Ok(state.candidates[index].clone())
        })
    }

    /// Deny exactly one candidate, by its request id. Denying a stranger's
    /// request leaves the code usable by the real phone. Denying an approved
    /// request that has not paired yet withdraws the approval, and then no
    /// candidate can use that code any more.
    pub fn deny(&self, request: &str, now: u64) -> Result<Candidate, String> {
        self.update(now, |state| {
            let index = find(state, request)?;
            let candidate = &mut state.candidates[index];
            match candidate.status {
                Status::Pending => {}
                Status::Approved if candidate.device.is_none() => {}
                Status::Approved | Status::Finalized => {
                    return Err(format!(
                        "pairing request {request} already paired its device; revoke the device instead"
                    ))
                }
                Status::Denied => return Err(format!("pairing request {request} was already denied")),
                Status::Superseded => {
                    return Err(format!(
                        "pairing request {request} can no longer be approved; there is nothing to deny"
                    ))
                }
            }
            if now >= candidate.expires {
                return Err(format!("pairing request {request} expired with its code"));
            }
            candidate.status = Status::Denied;
            candidate.decided_at = Some(now);
            Ok(candidate.clone())
        })
    }
}

fn find(state: &State, request: &str) -> Result<usize, String> {
    state
        .candidates
        .iter()
        .position(|c| c.request == request)
        .ok_or_else(|| format!("no pairing request has the id {request}"))
}

/// What a pairing QR code says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub version: u32,
    pub pc_id: String,
    pub pc_name: String,
    pub fingerprint: String,
    pub addresses: Vec<String>,
    pub code: String,
    pub expires: u64,
}

impl Payload {
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "{PREFIX}v={}&pc={}&n={}&fp={}",
            self.version,
            self.pc_id,
            percent_encode(&self.pc_name),
            self.fingerprint
        );
        for address in &self.addresses {
            text.push_str("&a=");
            text.push_str(&percent_encode(address));
        }
        text.push_str(&format!("&c={}&e={}", self.code, self.expires));
        text
    }

    /// Read pairing text, refusing anything malformed, from another version,
    /// or missing a field. Mirrors what the Android app accepts.
    pub fn parse(text: &str) -> Result<Payload, String> {
        let text = text.trim();
        if text.starts_with(LEGACY_PREFIX) {
            return Err(OLDER_FLOW.into());
        }
        let query = text
            .strip_prefix(PREFIX)
            .ok_or("this is not LCL pairing text")?;
        let mut version = None;
        let mut pc_id = None;
        let mut pc_name = None;
        let mut fingerprint = None;
        let mut addresses = Vec::new();
        let mut code = None;
        let mut expires = None;
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or("a pairing field has no value")?;
            let value = percent_decode(value).ok_or("a pairing field is badly encoded")?;
            let slot = match key {
                "v" => &mut version,
                "pc" => &mut pc_id,
                "n" => &mut pc_name,
                "fp" => &mut fingerprint,
                "c" => &mut code,
                "e" => &mut expires,
                "a" => {
                    addresses.push(value);
                    continue;
                }
                _ => continue, // a later version's extra field
            };
            if slot.replace(value).is_some() {
                return Err(format!("the pairing text names {key} twice"));
            }
        }
        let version: u32 = version
            .ok_or("no version")?
            .parse()
            .map_err(|_| "a bad version")?;
        if version < PAYLOAD_VERSION {
            return Err(OLDER_FLOW.into());
        }
        if version != PAYLOAD_VERSION {
            return Err(format!("pairing text version {version} is not supported"));
        }
        let fingerprint = fingerprint.ok_or("no PC fingerprint")?;
        if fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("the PC fingerprint is not 64 lowercase hex digits".into());
        }
        let code = code.ok_or("no pairing code")?;
        if b64::decode(&code).map(|b| b.len()) != Some(32) {
            return Err("the pairing code is malformed".into());
        }
        if addresses.is_empty() {
            return Err("the pairing text names no address to reach the PC at".into());
        }
        Ok(Payload {
            version,
            pc_id: pc_id.ok_or("no PC id")?,
            pc_name: pc_name.unwrap_or_default(),
            fingerprint,
            addresses,
            code,
            expires: expires
                .ok_or("no expiry")?
                .parse()
                .map_err(|_| "a bad expiry")?,
        })
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:[]".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairing(name: &str) -> (Pairing, Registry, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("lcl-remote-pairing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths::under(&root);
        (Pairing::new(&paths), Registry::new(&paths), root)
    }

    const PC: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn present(
        pairing: &Pairing,
        registry: &Registry,
        code: &str,
        fingerprint: &str,
        now: u64,
    ) -> Result<Presented, Refusal> {
        pairing.present(
            code,
            fingerprint,
            "Phone",
            PC,
            registry,
            "lcl.remote/1",
            now,
        )
    }

    fn pending(presented: Result<Presented, Refusal>) -> Candidate {
        match presented {
            Ok(Presented::Pending(candidate)) => candidate,
            other => panic!("not pending: {other:?}"),
        }
    }

    /// A11-R10: the exact values the Android app is tested against too.
    #[test]
    fn verification_codes_match_the_published_vectors() {
        let code = b64::encode(&[7u8; 32]);
        assert_eq!(code, "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc");
        let hash = lcl_spec::sha256::hex_digest(code.as_bytes());
        assert_eq!(
            hash,
            "dc4bf80c77473d130fa0de86ba4018fe98bb214005e6a5891d12ba91446f9e81"
        );
        assert_eq!(
            verification(&"a".repeat(64), &hash, &"b".repeat(64)),
            "edbb-8bd3-ad82"
        );
        let code: Vec<u8> = (0u8..32).collect();
        let code = b64::encode(&code);
        let hash = lcl_spec::sha256::hex_digest(code.as_bytes());
        let pc = lcl_spec::sha256::hex_digest(b"pc");
        assert_eq!(
            verification(&pc, &hash, &lcl_spec::sha256::hex_digest(b"phone")),
            "bd05-957e-fdfc"
        );
        assert_eq!(
            verification(&pc, &hash, &lcl_spec::sha256::hex_digest(b"another phone")),
            "d81e-cb19-d16e"
        );
    }

    #[test]
    fn a_code_only_ever_makes_a_pending_candidate_and_is_never_stored() {
        let (pairing, registry, root) = pairing("pending");
        let (challenge, code) = pairing.create(1_000, 300).unwrap();
        let first = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        assert_eq!(first.status, Status::Pending);
        assert_eq!(
            first.verification,
            verification(PC, &challenge.hash, &"a".repeat(64))
        );
        // Asked again: the same request, not a second one.
        let again = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_050));
        assert_eq!(again.request, first.request);
        assert_eq!(pairing.candidates(1_050).unwrap().len(), 1);
        assert!(registry.list().unwrap().is_empty());
        // Only a hash is stored.
        let stored = std::fs::read_to_string(root.join("state/lcl/remote/pairing.json")).unwrap();
        assert!(!stored.contains(&code));
        // Unknown and expired codes make nothing.
        assert_eq!(
            present(&pairing, &registry, "not-a-code", &"a".repeat(64), 1_001),
            Err(Refusal::Unknown)
        );
        assert_eq!(
            present(&pairing, &registry, &code, &"b".repeat(64), 1_300),
            Err(Refusal::Expired)
        );
        assert_eq!(
            present(&pairing, &registry, &code, &"a".repeat(64), 1_300),
            Err(Refusal::Expired)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn approval_trusts_one_certificate_once_and_spends_the_code() {
        let (pairing, registry, root) = pairing("approve");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        let b = pending(present(&pairing, &registry, &code, &"b".repeat(64), 1_002));
        pairing.approve(&a.request, 1_010).unwrap();
        // B was superseded, and cannot be approved or finish.
        assert!(pairing.approve(&b.request, 1_011).is_err());
        assert_eq!(
            present(&pairing, &registry, &code, &"b".repeat(64), 1_012),
            Err(Refusal::Used)
        );
        let Ok(Presented::Paired(device)) =
            present(&pairing, &registry, &code, &"a".repeat(64), 1_020)
        else {
            panic!("the approved candidate did not pair")
        };
        assert_eq!(device.fingerprint, "a".repeat(64));
        assert_eq!(registry.authorize(&"a".repeat(64)).unwrap().id, device.id);
        // Asked again after it paired: the same device, not another.
        assert_eq!(
            present(&pairing, &registry, &code, &"a".repeat(64), 1_021),
            Ok(Presented::Paired(device.clone()))
        );
        assert_eq!(registry.list().unwrap().len(), 1);
        // A new certificate with the spent code gets nothing.
        assert_eq!(
            present(&pairing, &registry, &code, &"d".repeat(64), 1_022),
            Err(Refusal::Used)
        );
        // Revoked: the old request does not bring it back.
        registry.revoke(&device.id, 1_030).unwrap();
        assert_eq!(
            present(&pairing, &registry, &code, &"a".repeat(64), 1_031),
            Err(Refusal::Used)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_crash_between_approval_and_trust_is_finished_only_by_the_approved_certificate() {
        let (pairing, registry, root) = pairing("crash");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        pending(present(&pairing, &registry, &code, &"b".repeat(64), 1_002));
        pairing.approve(&a.request, 1_010).unwrap();
        // Step 1 happened and the process died: the id is reserved, nothing
        // is trusted yet. Even after the code's expiry only A can finish.
        pairing
            .update(1_011, |state| {
                state.candidates[0].device = Some("0011223344556677".into());
                Ok::<_, String>(())
            })
            .unwrap();
        assert!(registry.list().unwrap().is_empty());
        assert_eq!(
            present(&pairing, &registry, &code, &"b".repeat(64), 1_400),
            Err(Refusal::Used)
        );
        assert!(pairing.approve(&a.request, 1_012).is_err());
        // Step 2 happened too: the record exists; finishing does not double it.
        registry
            .enroll(
                "0011223344556677",
                "Phone",
                &"a".repeat(64),
                "lcl.remote/1",
                1_013,
            )
            .unwrap();
        let Ok(Presented::Paired(device)) =
            present(&pairing, &registry, &code, &"a".repeat(64), 1_400)
        else {
            panic!("the approved candidate could not finish")
        };
        assert_eq!(device.id, "0011223344556677");
        assert_eq!(registry.list().unwrap().len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn denial_is_final_for_its_candidate_and_leaves_the_code_to_others() {
        let (pairing, registry, root) = pairing("deny");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        let stranger = pending(present(&pairing, &registry, &code, &"e".repeat(64), 1_001));
        pairing.deny(&stranger.request, 1_002).unwrap();
        assert_eq!(
            present(&pairing, &registry, &code, &"e".repeat(64), 1_003),
            Err(Refusal::Denied),
            "a denied candidate came back"
        );
        assert!(pairing.approve(&stranger.request, 1_004).is_err());
        let phone = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_005));
        pairing.approve(&phone.request, 1_006).unwrap();
        assert!(matches!(
            present(&pairing, &registry, &code, &"a".repeat(64), 1_007),
            Ok(Presented::Paired(_))
        ));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn withdrawing_an_approval_leaves_the_code_to_nobody() {
        let (pairing, registry, root) = pairing("withdraw");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        pairing.approve(&a.request, 1_002).unwrap();
        pairing.deny(&a.request, 1_003).unwrap();
        assert_eq!(
            present(&pairing, &registry, &code, &"a".repeat(64), 1_004),
            Err(Refusal::Denied)
        );
        assert_eq!(
            present(&pairing, &registry, &code, &"b".repeat(64), 1_005),
            Err(Refusal::Used)
        );
        assert!(registry.list().unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_expired_code_approves_nothing() {
        let (pairing, registry, root) = pairing("expired");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        assert!(pairing.approve(&a.request, 1_300).is_err());
        let (_, code) = pairing.create(2_000, 300).unwrap();
        let b = pending(present(&pairing, &registry, &code, &"b".repeat(64), 2_001));
        pairing.approve(&b.request, 2_002).unwrap();
        // Approved, but the phone did not come back before the code expired.
        assert_eq!(
            present(&pairing, &registry, &code, &"b".repeat(64), 2_300),
            Err(Refusal::Expired)
        );
        assert!(registry.list().unwrap().is_empty());
        assert!(pairing.candidates(2_300).unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn waiting_candidates_are_bounded() {
        let (pairing, registry, root) = pairing("bounded");
        let fingerprint = |i: usize| format!("{i:064x}");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        for i in 0..MAX_PENDING_PER_CHALLENGE {
            pending(present(&pairing, &registry, &code, &fingerprint(i), 1_001));
        }
        assert_eq!(
            present(&pairing, &registry, &code, &fingerprint(99), 1_002),
            Err(Refusal::Busy)
        );
        // The ones already waiting are still answered, and not doubled.
        pending(present(&pairing, &registry, &code, &fingerprint(0), 1_003));
        assert_eq!(
            pairing.candidates(1_003).unwrap().len(),
            MAX_PENDING_PER_CHALLENGE
        );
        // Across codes, the whole PC is bounded too.
        let mut n = 100;
        let mut refused = false;
        for _ in 0..MAX_PENDING / MAX_PENDING_PER_CHALLENGE + 1 {
            let (_, code) = pairing.create(1_000, 300).unwrap();
            for _ in 0..MAX_PENDING_PER_CHALLENGE {
                n += 1;
                if present(&pairing, &registry, &code, &fingerprint(n), 1_004) == Err(Refusal::Busy)
                {
                    refused = true;
                }
            }
        }
        assert!(refused);
        assert_eq!(pairing.candidates(1_004).unwrap().len(), MAX_PENDING);
        // Once the codes expire, their candidates are pruned on the next write.
        pairing.create(2_000, 300).unwrap();
        let stored = pairing.load().unwrap();
        assert!(stored.candidates.is_empty(), "expired candidates were kept");
        assert!(registry.list().unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unreadable_pairing_state_refuses_everything() {
        let (pairing, registry, root) = pairing("corrupt");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        let file = root.join("state/lcl/remote/pairing.json");
        for broken in [
            "{broken".to_string(),
            "{\"version\":3,\"challenges\":[],\"candidates\":[]}".to_string(),
            std::fs::read_to_string(&file)
                .unwrap()
                .replace("\"pending\"", "\"trusted\""),
        ] {
            std::fs::write(&file, broken).unwrap();
            assert!(matches!(
                present(&pairing, &registry, &code, &"a".repeat(64), 1_002),
                Err(Refusal::Unreadable(_))
            ));
            assert!(pairing.approve(&a.request, 1_002).is_err());
            assert!(pairing.candidates(1_002).is_err());
        }
        assert!(registry.list().unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_version_1_state_file_is_still_read() {
        let (pairing, registry, root) = pairing("v1");
        let file = root.join("state/lcl/remote/pairing.json");
        paths::write_private(
            &file,
            b"{\"version\":1,\"challenges\":[{\"id\":\"00\",\"hash\":\"00\",\"created\":1,\"expires\":2,\"consumed_at\":1,\"consumed_by\":\"x\"}]}",
        )
        .unwrap();
        let (_, code) = pairing.create(1_000, 300).unwrap();
        pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pairing_text_round_trips_and_malformed_or_older_text_is_refused() {
        let payload = Payload {
            version: 2,
            pc_id: "0123456789abcdef0123456789abcdef".into(),
            pc_name: "Aivars' PC & more | yes".into(),
            fingerprint: "a".repeat(64),
            addresses: vec!["192.168.1.20:47300".into(), "[fe80::1]:47300".into()],
            code: b64::encode(&[7u8; 32]),
            expires: 1_790_000_000,
        };
        let good = payload.to_text();
        assert!(good.starts_with("LCLPAIR|v=2&"), "{good}");
        assert!(!good.contains("://"), "{good}");
        assert_eq!(good.matches('|').count(), 1, "{good}");
        assert!(good.is_ascii());
        assert_eq!(Payload::parse(&good).unwrap(), payload);
        for bad in [
            "https://example.com/pair?v=2".to_string(),
            good.replace("LCLPAIR|", "LCLPAIR:"),
            good.replace("LCLPAIR|", "lclpair|"),
            good.replace("v=2", "v=3"),
            good.replace(&"a".repeat(64), &"A".repeat(64)),
            good.replace(&"a".repeat(64), &"a".repeat(63)),
            good.replace(&payload.code, "short"),
            good.replace("&a=192.168.1.20:47300&a=[fe80::1]:47300", ""),
            format!("{good}&c=again"),
            good.replace("e=1790000000", "e=soon"),
        ] {
            assert!(Payload::parse(&bad).is_err(), "{bad} was accepted");
        }
        // The earlier flow, as a link or as version 1 text, is refused with
        // a reason a person can act on.
        let legacy = good
            .replace("LCLPAIR|", "lclpair://pair?")
            .replace("v=2", "v=1");
        assert_eq!(Payload::parse(&legacy).unwrap_err(), OLDER_FLOW);
        assert_eq!(
            Payload::parse(&good.replace("v=2", "v=1")).unwrap_err(),
            OLDER_FLOW
        );
    }
}
