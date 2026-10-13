//! Shared HTTP plumbing for hyper-messenger providers.

mod client;
mod sidecar;
mod status;
mod url;

pub use client::{HttpClient, MAX_RETRIES, Request};
pub use sidecar::{DEFAULT_TIMEOUT, JsonRpcProcess, SidecarSpec};
pub use status::{error_message, map_http_status};
pub use url::{QueryString, query, urlencode};
