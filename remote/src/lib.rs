//! # lcl-remote — the PC side of LCL for Android
//!
//! A paired Android device is a second screen for this PC's LCL. The PC stays
//! the only authority: projects, files, the specification packages, the
//! engine, every check, inspection and run, every capability decision and all
//! evidence live here. The device shows them, edits documents, and asks.
//!
//! ## What this crate is, and is not
//!
//! It is a transport and a trust store. It holds the PC's identity
//! ([`identity`]), the devices it trusts ([`devices`]), one-time pairing
//! ([`pairing`]), and the encrypted, mutually authenticated connection
//! ([`tls`], [`session`]). It is **not** a second implementation of anything
//! about LCL: every document and engine operation a device asks for is one
//! of the desktop workspace's own routes, called in process
//! ([`lcl_workspace::Routes`]), so a device gets exactly the answers — and the
//! refusals — the desktop workspace gives.
//!
//! It is also not the desktop workspace's server. That one binds loopback
//! only and trusts a per-launch token; nothing about it is exposed here.

pub mod b64;
pub mod config;
pub mod der;
pub mod devices;
pub mod frame;
pub mod identity;
pub mod pairing;
pub mod paths;
pub mod projects;
pub mod qr;
pub mod service;
pub mod session;
pub mod tls;
