//! ACP JSON-RPC client over stdio (design §7–8)

mod client;
mod types;

pub use client::AcpClient;
pub use types::*;
