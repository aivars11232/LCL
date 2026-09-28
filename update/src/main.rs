//! `lcl-update`: check for, download and install LCL product updates.
//!
//!   lcl-update version                    this updater's product version
//!   lcl-update source                     where updates come from, and the keys trusted
//!   lcl-update status [--json]            what the last check found; never uses the network
//!   lcl-update check [--json]             ask the release source now
//!   lcl-update download [--json]          fetch, verify and stage the update found
//!   lcl-update apply [--wait-pid PID] [--relaunch]
//!                                         install the staged update, with rollback
//!   lcl-update verify [--json] MANIFEST SIGNATURE
//!                                         check a manifest against the trusted keys;
//!                                         with --json, what the verified manifest says
//!   lcl-update published                  the source's latest stable release, verified,
//!                                         as JSON; for the release builder
//!
//! Exit status: 0 when the action completed, 1 when it was refused or failed
//! (the state says why), 2 for a usage error.

use lcl_update::check::{self, Context};
use lcl_update::json::Object;
use lcl_update::state::{self, State};
use lcl_update::{apply, stage, trust};
use std::process::ExitCode;

const USAGE: &str = "usage: lcl-update version | source | status [--json] | check [--json] | \
download [--json] | apply [--wait-pid PID] [--relaunch] | verify [--json] MANIFEST SIGNATURE | \
published";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    match args.first().map(String::as_str) {
        Some("version") | Some("--version") => {
            println!("lcl-update {}", lcl_update::PRODUCT_VERSION);
            ExitCode::SUCCESS
        }
        Some("source") => source(),
        Some("status") => with_context(|ctx| {
            print(ctx, &state::load(&ctx.paths), json);
            ExitCode::SUCCESS
        }),
        Some("check") => locked(json, check::check),
        Some("download") => locked(json, |ctx| {
            let paths = ctx.paths.clone();
            stage::download(ctx, &mut |s| {
                let _ = state::save(&paths, s);
            })
        }),
        Some("apply") => {
            let mut wait_pid = None;
            let mut relaunch = false;
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--wait-pid" => match rest.next().and_then(|p| p.parse().ok()) {
                        Some(pid) => wait_pid = Some(pid),
                        None => return usage(),
                    },
                    "--relaunch" => relaunch = true,
                    "--json" => {}
                    _ => return usage(),
                }
            }
            locked(json, |ctx| {
                let paths = ctx.paths.clone();
                apply::apply(ctx, wait_pid, relaunch, &mut |s| {
                    let _ = state::save(&paths, s);
                })
            })
        }
        Some("published") => with_context(published),
        Some("verify") => match args[1..]
            .iter()
            .filter(|a| *a != "--json")
            .collect::<Vec<_>>()[..]
        {
            [manifest, signature] => verify(manifest, signature, json),
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn with_context(act: impl FnOnce(&Context) -> ExitCode) -> ExitCode {
    match Context::from_env() {
        Ok(ctx) => act(&ctx),
        Err(why) => {
            eprintln!("lcl-update: {why}");
            ExitCode::FAILURE
        }
    }
}

/// Run one action under the updater lock, record the state it ends in and
/// print it.
fn locked(json: bool, act: impl FnOnce(&Context) -> State) -> ExitCode {
    with_context(|ctx| {
        let (state, ok) = match state::lock(&ctx.paths) {
            Ok(_lock) => {
                let state = act(ctx);
                let _ = state::save(&ctx.paths, &state);
                let ok = state.state != "failed" && state.state != "offline";
                (state, ok)
            }
            Err(why) => (state::load(&ctx.paths).failed("busy", why), false),
        };
        print(ctx, &state, json);
        if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    })
}

fn print(ctx: &Context, state: &State, json: bool) {
    if json {
        let answer = Object::new()
            .raw("state", state.to_json())
            .str("installed", lcl_update::PRODUCT_VERSION)
            .bool("check_due", state.check_due(state::now()))
            .bool("configured", !ctx.keys.is_empty())
            .str("source", &ctx.source.describe())
            .finish();
        println!("{answer}");
        return;
    }
    println!("installed: LCL {}", lcl_update::PRODUCT_VERSION);
    println!(
        "state:     {}",
        if state.state.is_empty() {
            "never checked"
        } else {
            &state.state
        }
    );
    if let Some(available) = &state.available {
        println!(
            "available: LCL {} ({} bytes), published {}",
            available.version, available.size, available.published_at
        );
    }
    if let Some((kind, message)) = &state.error {
        println!("problem:   {kind}: {message}");
    }
}

fn source() -> ExitCode {
    with_context(|ctx| {
        println!("source: {}", ctx.source.describe());
        if cfg!(feature = "test-endpoint") {
            println!("build:  TEST (accepts a test server and test keys)");
        } else {
            println!("build:  release");
        }
        if ctx.keys.is_empty() {
            println!("keys:   none ({})", trust::NOT_CONFIGURED);
        }
        for key in &ctx.keys {
            println!("key:    {}", key.id);
        }
        ExitCode::SUCCESS
    })
}

/// The latest stable release of the pinned source, verified as a check
/// verifies it: `{"release": null}` when there is none. The release builder
/// takes release history from this. An unreadable listing, a release without a
/// verified manifest of its own, or a build that trusts no key fails.
fn published(ctx: &Context) -> ExitCode {
    let release = match ctx.source.latest() {
        Ok(Some(release)) => release,
        Ok(None) => {
            let answer = Object::new()
                .raw("release", "null".to_string())
                .str("source", &ctx.source.describe())
                .finish();
            println!("{answer}");
            return ExitCode::SUCCESS;
        }
        Err(failure) => {
            eprintln!("lcl-update: {}: {failure}", ctx.source.describe());
            return ExitCode::FAILURE;
        }
    };
    match check::fetch_verified(ctx, &release) {
        Ok(check::Verified {
            bytes, manifest, ..
        }) => {
            let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
            let answer = Object::new()
                .str("release", &release.tag)
                .str("product_version", &manifest.product_version.to_string())
                .str("source_commit", &manifest.source_commit)
                .str("manifest_sha256", &trust::hex(digest.as_ref()))
                .str("source", &ctx.source.describe())
                .finish();
            println!("{answer}");
            ExitCode::SUCCESS
        }
        Err((_, why)) => {
            eprintln!("lcl-update: {why}");
            ExitCode::FAILURE
        }
    }
}

fn verify(manifest: &str, signature: &str, json: bool) -> ExitCode {
    let read = |path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"));
    let result = trust::trusted_keys().and_then(|keys| {
        let bytes = read(manifest)?;
        let manifest = check::verify(&bytes, &read(signature)?, &keys)?;
        Ok(manifest)
    });
    match result {
        Ok(manifest) if json => {
            // Only what a verified manifest says: the release builder believes
            // the previous release through this and nothing else.
            let android = Object::new()
                .str("application_id", &manifest.android.application_id)
                .str("version_name", &manifest.android.version_name)
                .num("version_code", manifest.android.version_code)
                .str("signer_sha256", &manifest.android.signer_sha256)
                .finish();
            let answer = Object::new()
                .str("product_version", &manifest.product_version.to_string())
                .str("release_tag", &manifest.release_tag)
                .str("source_commit", &manifest.source_commit)
                .str("signing_key_id", &manifest.signing_key_id)
                .raw("android", android)
                .finish();
            println!("{answer}");
            ExitCode::SUCCESS
        }
        Ok(manifest) => {
            println!(
                "verified: LCL {} signed by {}",
                manifest.product_version, manifest.signing_key_id
            );
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("lcl-update: {why}");
            ExitCode::FAILURE
        }
    }
}
