//! Request/response DTOs shared across the API surface.
//!
//! Grows phase by phase; every endpoint in `docs/API.md` gets its types
//! here, never redefined locally in controller, agent, or frontend.

mod agent;
mod error;
mod health;
mod instance;
mod inventory;
mod local_login;
mod me;
mod metrics;
mod project;
mod registry;
mod server;
mod server_detail;

pub use agent::*;
pub use error::*;
pub use health::*;
pub use instance::*;
pub use inventory::*;
pub use local_login::*;
pub use me::*;
pub use metrics::*;
pub use project::*;
pub use registry::*;
pub use server::*;
pub use server_detail::*;
