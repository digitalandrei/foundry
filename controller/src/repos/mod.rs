//! Data access, one module per aggregate. Functions take executors so
//! call sites compose them into transactions
//! (docs/RUST_RULES.md § sqlx / MySQL).

pub mod instances;
pub mod local_admins;
pub mod mirror;
pub mod servers;
pub mod users;
