//! A deterministic network capability.
//!
//! It reaches nothing. Each address is bound to the response it always returns,
//! so a transfer test is a pure function of its fixture rather than of whatever
//! the network did today.

use lcl_capabilities::net::{Address, NetError, Response, Transport};
use lcl_capabilities::Bounds;
use std::collections::BTreeMap;

/// One request this transport was asked to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    pub method: &'static str,
    pub uri: String,
    pub body: Vec<u8>,
}

/// An in-memory transport.
#[derive(Debug, Clone, Default)]
pub struct MemoryTransport {
    responses: BTreeMap<String, Response>,
    exchanges: Vec<Exchange>,
}

impl MemoryTransport {
    pub fn new() -> MemoryTransport {
        MemoryTransport::default()
    }

    /// Bind one absolute URI to the content it always returns.
    pub fn with_resource(
        mut self,
        uri: impl Into<String>,
        body: impl AsRef<[u8]>,
    ) -> MemoryTransport {
        self.responses.insert(
            uri.into(),
            Response {
                status: 200,
                body: body.as_ref().to_vec(),
            },
        );
        self
    }

    /// Bind one absolute URI to an exact status with no content.
    pub fn with_status(mut self, uri: impl Into<String>, status: u16) -> MemoryTransport {
        self.responses.insert(
            uri.into(),
            Response {
                status,
                body: Vec::new(),
            },
        );
        self
    }

    /// Every exchange this transport was asked for, in order.
    pub fn exchanges(&self) -> &[Exchange] {
        &self.exchanges
    }

    fn uri(address: &Address) -> String {
        format!("{}://{}{}", address.scheme, address.host, address.target)
    }

    fn answer(
        &mut self,
        method: &'static str,
        address: &Address,
        body: &[u8],
    ) -> Result<Response, NetError> {
        let uri = MemoryTransport::uri(address);
        self.exchanges.push(Exchange {
            method,
            uri: uri.clone(),
            body: body.to_vec(),
        });
        match self.responses.get(&uri) {
            Some(response) => Ok(response.clone()),
            // An address this fixture was never given is unreachable, which is
            // the truth: nothing here can answer for it.
            None => Err(NetError::Unreachable(format!("{uri} is not bound"))),
        }
    }
}

impl Transport for MemoryTransport {
    fn get(&mut self, address: &Address, _bounds: &Bounds) -> Result<Response, NetError> {
        self.answer("GET", address, &[])
    }

    fn put(
        &mut self,
        address: &Address,
        body: &[u8],
        _bounds: &Bounds,
    ) -> Result<Response, NetError> {
        self.answer("PUT", address, body)
    }
}
