//! The `lcl-remote` command: run the service, pair devices, manage them.

use lcl_protocol::json::{Node, Object};
use lcl_remote::config::Config;
use lcl_remote::devices::{Device, Registry};
use lcl_remote::identity::Identity;
use lcl_remote::pairing::{Link, Pairing, DEFAULT_TTL_SECONDS, LINK_VERSION};
use lcl_remote::paths::{self, Paths};
use lcl_remote::projects::{project_id, Projects, Specs};
use lcl_remote::service::{self, Options, Service};
use lcl_spec::json::Json;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
lcl-remote — the PC side of LCL for Android

USAGE:
    lcl-remote serve [--spec PATH] [--localized-spec PATH] [--listen ADDR]
                     [--port PORT] [--no-discovery]
        Run the service paired devices connect to. It listens on
        0.0.0.0:47300 by default and serves nothing to a device that has not
        paired. Every document, check and run goes through the same engine and
        rules as the desktop workspace.

    lcl-remote pair [--address HOST:PORT]... [--minutes N] [--json]
        Show a one-time QR code to pair an Android device. It is good for one
        device and 5 minutes. Addresses default to this PC's own, plus any
        added with `lcl-remote address add`.

    lcl-remote devices [--json]         List paired devices.
    lcl-remote revoke DEVICE-ID         Stop trusting one device. It is
                                        disconnected and must pair again.
    lcl-remote rename DEVICE-ID NAME    Rename one device.
    lcl-remote projects [--json]        List the projects devices can open.
    lcl-remote projects add PATH        Share one more project folder.
    lcl-remote projects remove PATH     Stop sharing one.
    lcl-remote address add HOST:PORT    An address that reaches this PC from
                                        other networks, for new pairing codes.
    lcl-remote address remove HOST:PORT
    lcl-remote identity [--json]        This PC's id, name and fingerprint.
    lcl-remote identity rename NAME
    lcl-remote status [--json]          Whether the service is running.

Everything is kept under ~/.config/lcl/remote and ~/.local/state/lcl/remote
(or the XDG directories), readable by you alone.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(detail) => {
            eprintln!("lcl-remote: {detail}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let paths = Paths::from_env()?;
    let (command, rest) = args
        .split_first()
        .map(|(c, r)| (c.as_str(), r))
        .unwrap_or(("help", &[]));
    let json = rest.iter().any(|a| a == "--json");
    match command {
        "serve" => serve(paths, rest),
        "pair" => pair(&paths, rest, json),
        "devices" => devices(&paths, json),
        "revoke" => {
            let id = rest.first().ok_or("revoke needs a device id")?;
            let device = Registry::new(&paths).revoke(id, paths::now())?;
            println!(
                "revoked {} ({}); it is disconnected and must pair again",
                device.name, device.id
            );
            Ok(())
        }
        "rename" => {
            let (Some(id), Some(name)) = (rest.first(), rest.get(1)) else {
                return Err("rename needs a device id and a name".into());
            };
            let device = Registry::new(&paths).rename(id, name)?;
            println!("renamed {} to {}", device.id, device.name);
            Ok(())
        }
        "projects" => projects(&paths, rest, json),
        "address" => addresses(&paths, rest),
        "identity" => identity(&paths, rest, json),
        "status" => status(&paths, json),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command {other}; see lcl-remote help")),
    }
}

fn value(rest: &[String], name: &str) -> Result<Option<String>, String> {
    match rest.iter().position(|a| a == name) {
        Some(i) => rest
            .get(i + 1)
            .cloned()
            .map(Some)
            .ok_or(format!("{name} needs a value")),
        None => Ok(None),
    }
}

fn values(rest: &[String], name: &str) -> Vec<String> {
    rest.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].clone())
        .collect()
}

fn serve(paths: Paths, rest: &[String]) -> Result<(), String> {
    let specs = Specs::locate(
        &paths,
        value(rest, "--spec")?.map(PathBuf::from),
        value(rest, "--localized-spec")?.map(PathBuf::from),
    )?;
    let port = value(rest, "--port")?
        .map(|p| {
            p.parse::<u16>()
                .map_err(|_| "--port needs a number".to_string())
        })
        .transpose()?;
    let service = Service::bind(Options {
        paths,
        specs,
        listen: value(rest, "--listen")?,
        port,
        discovery: !rest.iter().any(|a| a == "--no-discovery"),
    })?;
    println!("LCL remote service");
    println!(
        "  PC           {} ({})",
        service.identity().name,
        service.identity().pc_id
    );
    println!("  fingerprint  {}", service.identity().fingerprint);
    println!("  listening    {}", service.address());
    println!();
    println!("Pair a device with: lcl-remote pair");
    service.serve()
}

fn pair(paths: &Paths, rest: &[String], json: bool) -> Result<(), String> {
    let identity = Identity::load_or_create(paths)?;
    let config = Config::load(paths)?;
    let minutes: u64 = value(rest, "--minutes")?
        .map(|m| {
            m.parse()
                .map_err(|_| "--minutes needs a number".to_string())
        })
        .transpose()?
        .unwrap_or(DEFAULT_TTL_SECONDS / 60);
    if !(1..=60).contains(&minutes) {
        return Err("a pairing code lasts 1 to 60 minutes".into());
    }
    let mut addresses = values(rest, "--address");
    if addresses.is_empty() {
        // The port the running service really listens on, which `serve --port`
        // may have changed; the configured one when it is not running.
        let port = live_status(paths)
            .and_then(|status| status.get("port").and_then(Json::as_u64))
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(config.port);
        addresses = service::local_addresses()
            .into_iter()
            .map(|ip| format!("{ip}:{port}"))
            .collect();
        addresses.extend(config.public_addresses.iter().cloned());
    }
    if addresses.is_empty() {
        return Err(
            "this PC has no network address to put in a pairing code; pass --address HOST:PORT"
                .into(),
        );
    }
    let (challenge, code) = Pairing::new(paths).create(paths::now(), minutes * 60)?;
    let link = Link {
        version: LINK_VERSION,
        pc_id: identity.pc_id.clone(),
        pc_name: identity.name.clone(),
        fingerprint: identity.fingerprint.clone(),
        addresses: addresses.clone(),
        code,
        expires: challenge.expires,
    };
    let uri = link.to_uri();
    if json {
        println!(
            "{}",
            Object::new()
                .with("link", Node::string(&uri))
                .with("svg", Node::string(lcl_remote::qr::svg(&uri)?))
                .with("expires", Node::u64(challenge.expires))
                .with("addresses", Node::array(addresses.iter().map(Node::string)))
                .with("pc", Node::string(&identity.name))
                .with("fingerprint", Node::string(&identity.fingerprint))
                .pretty()
        );
        return Ok(());
    }
    println!("{}", lcl_remote::qr::terminal(&uri)?);
    println!("Scan this with LCL on your Android device: Pair a PC → Scan QR code.");
    println!(
        "It works once, for {minutes} minute(s). This PC is {}.",
        identity.name
    );
    // The same grouping the app shows before it pairs, to compare by eye.
    let groups: Vec<&str> = (0..8)
        .map(|i| &identity.fingerprint[i * 4..i * 4 + 4])
        .collect();
    println!("Fingerprint: {} …", groups.join(" "));
    println!("Addresses in the code: {}", addresses.join(", "));
    println!();
    println!("Or paste this link into the app:");
    println!("{uri}");
    Ok(())
}

/// The service's status, if it wrote one recently.
fn live_status(paths: &Paths) -> Option<Json> {
    let text = std::fs::read_to_string(paths.state.join("status.json")).ok()?;
    let status = lcl_spec::json::parse(&text).ok()?;
    let updated = status.get("updated").and_then(Json::as_u64)?;
    (paths::now().saturating_sub(updated) <= 10).then_some(status)
}

fn devices(paths: &Paths, json: bool) -> Result<(), String> {
    let devices = Registry::new(paths).list()?;
    let status = live_status(paths);
    let online = |device: &Device| {
        status
            .as_ref()
            .and_then(|s| s.get("sessions").and_then(Json::as_array))
            .is_some_and(|sessions| {
                sessions
                    .iter()
                    .any(|s| s.get("device").and_then(Json::as_str) == Some(device.id.as_str()))
            })
    };
    if json {
        let items = devices.iter().map(|d| {
            Object::new()
                .with("id", Node::string(&d.id))
                .with("name", Node::string(&d.name))
                .with("fingerprint", Node::string(&d.fingerprint))
                .with("paired_at", Node::u64(d.paired_at))
                .with(
                    "last_seen",
                    d.last_seen.map(Node::u64).unwrap_or(Node::Null),
                )
                .with(
                    "revoked_at",
                    d.revoked_at.map(Node::u64).unwrap_or(Node::Null),
                )
                .with("online", Node::Bool(online(d)))
                .into()
        });
        println!(
            "{}",
            Object::new()
                .with("service_running", Node::Bool(status.is_some()))
                .with("devices", Node::array(items))
                .pretty()
        );
        return Ok(());
    }
    if devices.is_empty() {
        println!("No devices are paired. Pair one with: lcl-remote pair");
    }
    for d in &devices {
        let state = if d.is_revoked() {
            "revoked"
        } else if online(d) {
            "online"
        } else {
            "offline"
        };
        println!(
            "{}  {:<24} {:<8} last connected {}",
            d.id,
            d.name,
            state,
            d.last_seen.map(ago).unwrap_or_else(|| "never".into())
        );
    }
    if status.is_none() {
        println!("(the service is not running: lcl-remote serve)");
    }
    Ok(())
}

fn ago(then: u64) -> String {
    let seconds = paths::now().saturating_sub(then);
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

fn projects(paths: &Paths, rest: &[String], json: bool) -> Result<(), String> {
    let mut config = Config::load(paths)?;
    match rest.first().map(String::as_str) {
        Some("add") => {
            let path = PathBuf::from(rest.get(1).ok_or("projects add needs a folder")?);
            let path = path
                .canonicalize()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if !path.is_dir() {
                return Err(format!("{} is not a folder", path.display()));
            }
            if !config.projects.contains(&path) {
                config.projects.push(path.clone());
                config.store(paths)?;
            }
            println!("shared {} as project {}", path.display(), project_id(&path));
            Ok(())
        }
        Some("remove") => {
            let target = rest
                .get(1)
                .ok_or("projects remove needs a folder or project id")?;
            let before = config.projects.len();
            config.projects.retain(|p| {
                let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
                p.display().to_string() != *target && project_id(&canonical) != *target
            });
            if config.projects.len() == before {
                return Err(format!("{target} is not a shared project"));
            }
            config.store(paths)?;
            println!("stopped sharing {target}");
            Ok(())
        }
        _ => {
            // Listing needs no engine: only the folders and their ids.
            let specs = Specs {
                core: PathBuf::new(),
                localized: None,
            };
            let list = Projects::new(specs, paths).list(paths, &config);
            if json {
                let items = list.iter().map(|p| {
                    Object::new()
                        .with("id", Node::string(&p.id))
                        .with("name", Node::string(&p.name))
                        .with("root", Node::string(p.root.display().to_string()))
                        .with("default", Node::Bool(p.default))
                        .into()
                });
                println!(
                    "{}",
                    Object::new().with("projects", Node::array(items)).pretty()
                );
            } else {
                for p in &list {
                    println!(
                        "{}  {}{}",
                        p.id,
                        p.root.display(),
                        if p.default {
                            "  (default workspace)"
                        } else {
                            ""
                        }
                    );
                }
            }
            Ok(())
        }
    }
}

fn addresses(paths: &Paths, rest: &[String]) -> Result<(), String> {
    let mut config = Config::load(paths)?;
    let address = rest.get(1).ok_or("address add|remove needs HOST:PORT")?;
    match rest.first().map(String::as_str) {
        Some("add") => {
            if !address
                .rsplit_once(':')
                .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok())
            {
                return Err(format!("{address} is not HOST:PORT"));
            }
            if !config.public_addresses.contains(address) {
                config.public_addresses.push(address.clone());
            }
        }
        Some("remove") => config.public_addresses.retain(|a| a != address),
        _ => return Err("address add|remove HOST:PORT".into()),
    }
    config.store(paths)?;
    println!(
        "pairing codes will offer: {}",
        config.public_addresses.join(", ")
    );
    Ok(())
}

fn identity(paths: &Paths, rest: &[String], json: bool) -> Result<(), String> {
    let mut identity = Identity::load_or_create(paths)?;
    if rest.first().map(String::as_str) == Some("rename") {
        identity.rename(paths, rest.get(1).ok_or("identity rename needs a name")?)?;
    }
    if json {
        println!(
            "{}",
            Object::new()
                .with("pc_id", Node::string(&identity.pc_id))
                .with("name", Node::string(&identity.name))
                .with("fingerprint", Node::string(&identity.fingerprint))
                .pretty()
        );
    } else {
        println!("PC           {}", identity.name);
        println!("id           {}", identity.pc_id);
        println!("fingerprint  {}", identity.fingerprint);
    }
    Ok(())
}

fn status(paths: &Paths, json: bool) -> Result<(), String> {
    let status = live_status(paths);
    if json {
        println!(
            "{}",
            Object::new()
                .with("running", Node::Bool(status.is_some()))
                .with(
                    "port",
                    status
                        .as_ref()
                        .and_then(|s| s.get("port").and_then(Json::as_u64))
                        .map(Node::u64)
                        .unwrap_or(Node::Null)
                )
                .pretty()
        );
    } else if let Some(status) = status {
        let sessions = status
            .get("sessions")
            .and_then(Json::as_array)
            .map(|s| s.len())
            .unwrap_or(0);
        println!(
            "running on port {}, {sessions} device(s) connected",
            status.get("port").and_then(Json::as_u64).unwrap_or(0)
        );
    } else {
        println!("not running; start it with: lcl-remote serve");
    }
    Ok(())
}
