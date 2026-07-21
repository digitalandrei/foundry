//! Shared application state injected into every handler.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sqlx::MySqlPool;

use crate::crypto::SecretBox;

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub secrets: SecretBox,
    /// Outbound client for GitLab API calls (rustls, timeouts set).
    pub http: reqwest::Client,
    pub public_url: Arc<str>,
    pub admin_emails: Arc<[String]>,
    /// `ai.protv.ro` — None disables HTTP/S publishing.
    pub apps_domain: Option<Arc<str>>,
    /// Live DEPLOY progress text by deployment id (agent posts every
    /// ~2s while pulling). Deliberately in-memory: it is transient by
    /// definition — the durable truth is the state machine; a restart
    /// just blanks the text until the next report. Lock is never held
    /// across an await (docs/RUST_RULES.md).
    pub progress: Arc<Mutex<HashMap<uuid::Uuid, String>>>,
    /// Pending/active interactive shell sessions, keyed by session id
    /// (crate::shell). In-memory by nature — a session is a live socket
    /// pair, meaningless across a restart. Same lock discipline as
    /// `progress`: short sync sections only.
    pub shells: crate::shell::ShellRegistry,
    /// Pending/active placement-volume file sessions (crate::files). The
    /// approved roots and live socket channels are transient and become
    /// meaningless across a controller restart.
    pub files: crate::files::FileRegistry,
}

/// Lock an in-memory cache mutex, recovering the guard if a previous holder
/// panicked. `progress`, `shells`, and `files` are ephemeral caches (transient by
/// definition — see their field docs); a poisoned lock must never cascade
/// into failing every progress poll or shell session for the life of the
/// process. The protected data is a plain map, so recovering it is safe.
pub fn lock_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::lock_recover;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[test]
    fn lock_recover_survives_poison() {
        let m: Mutex<HashMap<u32, u32>> = Mutex::new(HashMap::new());
        // Poison the lock by panicking while holding the guard.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut g = m.lock().unwrap();
            g.insert(1, 1);
            panic!("poison the lock");
        }));
        assert!(m.is_poisoned());
        // A plain `.lock().expect()` would panic here; the helper recovers.
        let mut g = lock_recover(&m);
        assert_eq!(g.get(&1), Some(&1)); // data written before the panic survives
        g.insert(2, 2);
        assert_eq!(g.get(&2), Some(&2));
    }
}
