//! Engram IPC Protocol and Client/Server
//!
//! This crate provides the IPC protocol definitions and Unix socket
//! client/server implementations for communication with the Engram daemon.

mod client;
mod error;
mod protocol;
mod server;

pub use client::IpcClient;
pub use error::IpcError;
pub use protocol::*;
pub use server::{IpcServer, RequestHandler};
