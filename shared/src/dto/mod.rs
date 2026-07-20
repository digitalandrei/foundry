//! Request/response DTOs shared across the API surface.
//!
//! Grows phase by phase; every endpoint in `docs/API.md` gets its types
//! here, never redefined locally in controller, agent, or frontend.

mod agent;
mod audit;
mod deployment;
mod error;
mod files;
mod gpu_group;
mod health;
mod instance;
mod inventory;
mod local_login;
mod logs;
mod me;
mod metrics;
mod project;
mod registry;
mod server;
mod server_detail;
mod shell;
mod task;

pub use agent::*;
pub use audit::*;
pub use deployment::*;
pub use error::*;
pub use files::*;
pub use gpu_group::*;
pub use health::*;
pub use instance::*;
pub use inventory::*;
pub use local_login::*;
pub use logs::*;
pub use me::*;
pub use metrics::*;
pub use project::*;
pub use registry::*;
pub use server::*;
pub use server_detail::*;
pub use shell::*;
pub use task::*;
