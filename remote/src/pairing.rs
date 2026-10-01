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
use ring::rand::SystemRandom;
use std::path::PathBuf;

mod payload;
mod state;
#[cfg(test)]
mod tests;

pub use payload::{Payload, OLDER_FLOW, PAYLOAD_VERSION, PREFIX};
use state::State;

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
                "too many pairing requests are waiting on this PC; \
                 deny the ones you do not recognise, or show a new QR code",
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
            let candidate = self.enlist(
                &mut state,
                &challenge,
                fingerprint,
                name,
                pc_fingerprint,
                now,
            )?;
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

    /// Record a device that presents a live code for the first time as a
    /// pending candidate. Nothing about it is trusted.
    fn enlist(
        &self,
        state: &mut State,
        challenge: &Challenge,
        fingerprint: &str,
        name: &str,
        pc_fingerprint: &str,
        now: u64,
    ) -> Result<Candidate, Refusal> {
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
        self.store(state).map_err(Refusal::Unreadable)?;
        Ok(candidate)
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
            let taken = || {
                format!(
                    "another device was approved with the same pairing code; \
                     request {request} can no longer be approved"
                )
            };
            match candidate.status {
                Status::Pending => {}
                Status::Approved => {
                    return Err(format!("pairing request {request} is already approved"))
                }
                Status::Denied => return Err(format!("pairing request {request} was denied")),
                Status::Finalized => {
                    return Err(format!(
                        "pairing request {request} already paired its device"
                    ))
                }
                Status::Superseded => return Err(taken()),
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
                return Err(taken());
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
                        "pairing request {request} already paired its device; \
                         revoke the device instead"
                    ))
                }
                Status::Denied => {
                    return Err(format!("pairing request {request} was already denied"))
                }
                Status::Superseded => {
                    return Err(format!(
                        "pairing request {request} can no longer be approved; \
                         there is nothing to deny"
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
