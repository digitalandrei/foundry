//! Foundry wire contract: state enums, ID newtypes, and shared DTOs.
//!
//! Single source of truth shared by `foundry-controller` and
//! `foundry-agent` (and mirrored by the frontend's `lib/states.ts`).
//! State machines are documented in `docs/ARCHITECTURE.md`; the string
//! forms here are exactly what the database stores (`docs/DATABASE.md`).

pub mod dto;
pub mod ids;
pub mod states;

pub use ids::*;
pub use states::*;
