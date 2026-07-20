//! Data access, one module per aggregate. Functions take executors so
//! call sites compose them into transactions
//! (docs/RUST_RULES.md § sqlx / MySQL).

mod deployment_adoption;
mod deployment_queries;
mod deployment_targets;
pub mod deployments;
pub mod gpu_groups;
pub mod instances;
pub mod inventory;
pub mod local_admins;
pub mod logs;
pub mod metrics;
pub mod mirror;
pub mod servers;
pub mod slots;
pub mod tasks;
pub mod users;
pub mod volumes;
