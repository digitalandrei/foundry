//! Request/response DTOs shared across the API surface.
//!
//! Grows phase by phase; every endpoint in `docs/API.md` gets its types
//! here, never redefined locally in controller, agent, or frontend.

mod error;
mod health;
mod instance;
mod me;
mod project;
mod registry;

pub use error::*;
pub use health::*;
pub use instance::*;
pub use me::*;
pub use project::*;
pub use registry::*;
