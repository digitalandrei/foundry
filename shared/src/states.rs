//! State enums for every Foundry state machine.
//!
//! The `&'static str` forms are the canonical wire/database encoding:
//! the database stores them verbatim in VARCHAR state columns and the
//! API serializes them via serde. Adding a variant requires a matching
//! migration note in `docs/DATABASE.md` and, for slot/deployment states,
//! an entry in the frontend state map (`frontend/src/lib/states.ts`).

/// Failure to parse a state string that did not match any variant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {type_name}: {value:?}")]
pub struct ParseEnumError {
    pub type_name: &'static str,
    pub value: String,
}

/// Defines a copyable enum with a fixed string form per variant,
/// wired for serde, `Display`, `FromStr`, and exhaustive iteration.
macro_rules! str_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident { $($variant:ident => $str:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash,
            serde::Serialize, serde::Deserialize,
        )]
        pub enum $name {
            $(#[serde(rename = $str)] $variant),+
        }

        impl $name {
            /// Every variant, for exhaustive tests and UI legends.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The canonical wire/database string.
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $str),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($str => Ok(Self::$variant),)+
                    _ => Err(ParseEnumError {
                        type_name: stringify!($name),
                        value: s.to_string(),
                    }),
                }
            }
        }
    };
}

str_enum! {
    /// What a schedulable slot physically is.
    pub enum SlotType {
        FullGpu => "FULL_GPU",
        MigSlot => "MIG_SLOT",
    }
}

str_enum! {
    /// Slot scheduling state (`docs/ARCHITECTURE.md` § Slot states).
    pub enum SlotState {
        Free => "FREE",
        Reserved => "RESERVED",
        Deploying => "DEPLOYING",
        Running => "RUNNING",
        Failed => "FAILED",
        Stopping => "STOPPING",
        Offline => "OFFLINE",
    }
}

str_enum! {
    /// Deployment lifecycle (`docs/ARCHITECTURE.md` § Deployment Lifecycle).
    pub enum DeploymentState {
        Pending => "PENDING",
        Validating => "VALIDATING",
        Prepared => "PREPARED",
        PullingImage => "PULLING_IMAGE",
        CreatingContainer => "CREATING_CONTAINER",
        Starting => "STARTING",
        WaitingHealth => "WAITING_HEALTH",
        Publishing => "PUBLISHING",
        PublishFailed => "PUBLISH_FAILED",
        Running => "RUNNING",
        Stopping => "STOPPING",
        Stopped => "STOPPED",
        Restarting => "RESTARTING",
        Removing => "REMOVING",
        Removed => "REMOVED",
        Failed => "FAILED",
        Replaced => "REPLACED",
    }
}

str_enum! {
    /// Work items agents poll for (`docs/ARCHITECTURE.md` § Agent Tasks).
    /// REMOVE_VOLUME/PURGE_VOLUMES are persistent-storage amendments.
    pub enum TaskType {
        DeployContainer => "DEPLOY_CONTAINER",
        PrepareDeploy => "PREPARE_DEPLOY",
        QuiesceContainer => "QUIESCE_CONTAINER",
        RollbackContainer => "ROLLBACK_CONTAINER",
        StopContainer => "STOP_CONTAINER",
        RestartContainer => "RESTART_CONTAINER",
        RemoveContainer => "REMOVE_CONTAINER",
        RemoveVolume => "REMOVE_VOLUME",
        PurgeVolumes => "PURGE_VOLUMES",
        RefreshInventory => "REFRESH_INVENTORY",
        UploadLogs => "UPLOAD_LOGS",
        PublishVhost => "PUBLISH_VHOST",
        UpgradeAgent => "UPGRADE_AGENT",
    }
}

str_enum! {
    /// Where a persistent volume may be mounted on its server.
    pub enum VolumePlacement {
        Slot => "SLOT",
        Server => "SERVER",
    }
}

#[allow(clippy::derivable_impls)]
impl Default for VolumePlacement {
    fn default() -> Self {
        Self::Slot
    }
}

str_enum! {
    /// Queue state of an agent task.
    pub enum TaskState {
        Queued => "QUEUED",
        Dispatched => "DISPATCHED",
        Succeeded => "SUCCEEDED",
        Failed => "FAILED",
        Cancelled => "CANCELLED",
    }
}

str_enum! {
    /// Server liveness as derived from heartbeats.
    pub enum ServerStatus {
        Online => "ONLINE",
        Offline => "OFFLINE",
        Degraded => "DEGRADED",
    }
}

str_enum! {
    /// Who caused a state transition or audited action.
    pub enum ActorType {
        User => "USER",
        Agent => "AGENT",
        Controller => "CONTROLLER",
    }
}

str_enum! {
    /// How a published port is exposed (plans/phase-06.md § Networking).
    /// HTTP/HTTPS are published via the per-server agent-managed nginx
    /// vhost (amendment 0.8.0); TCP/UDP map directly onto the server IP.
    pub enum PortKind {
        Http => "HTTP",
        Https => "HTTPS",
        Tcp => "TCP",
        Udp => "UDP",
    }
}

impl PortKind {
    /// The L4 protocol Docker publishes for this kind.
    pub fn protocol(self) -> &'static str {
        match self {
            PortKind::Udp => "udp",
            _ => "tcp",
        }
    }
}

/// Matches the pre-`kind` wire behavior so task payloads queued by an
/// older controller stay deserializable across an upgrade (review
/// finding: `kind` is `#[serde(default)]` in PortBinding).
impl Default for PortKind {
    fn default() -> Self {
        PortKind::Tcp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn round_trip<T>()
    where
        T: Copy
            + PartialEq
            + std::fmt::Debug
            + std::fmt::Display
            + FromStr<Err = ParseEnumError>
            + serde::Serialize
            + serde::de::DeserializeOwned
            + 'static,
        T: AsAll,
    {
        for &v in T::all() {
            // string round trip
            let parsed = T::from_str(&v.to_string()).expect("parse own string");
            assert_eq!(parsed, v);
            // serde round trip, and serde form == Display form
            let json = serde_json::to_string(&v).expect("serialize");
            assert_eq!(json, format!("\"{v}\""));
            let back: T = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, v);
        }
        assert!(T::from_str("definitely-not-a-state").is_err());
    }

    trait AsAll: Sized + 'static {
        fn all() -> &'static [Self];
    }
    macro_rules! impl_as_all {
        ($($t:ty),+) => { $(impl AsAll for $t { fn all() -> &'static [Self] { Self::ALL } })+ };
    }
    impl_as_all!(
        SlotType,
        SlotState,
        DeploymentState,
        TaskType,
        TaskState,
        ServerStatus,
        ActorType,
        PortKind,
        VolumePlacement
    );

    #[test]
    fn all_enums_round_trip() {
        round_trip::<SlotType>();
        round_trip::<SlotState>();
        round_trip::<DeploymentState>();
        round_trip::<TaskType>();
        round_trip::<TaskState>();
        round_trip::<ServerStatus>();
        round_trip::<ActorType>();
        round_trip::<PortKind>();
        round_trip::<VolumePlacement>();
    }

    #[test]
    fn strings_are_unique_within_each_enum() {
        fn assert_unique<T: Copy + 'static + AsAll + std::fmt::Display>() {
            let mut seen = std::collections::HashSet::new();
            for &v in T::all() {
                assert!(seen.insert(v.to_string()), "duplicate string {v}");
            }
        }
        assert_unique::<SlotType>();
        assert_unique::<SlotState>();
        assert_unique::<DeploymentState>();
        assert_unique::<TaskType>();
        assert_unique::<TaskState>();
        assert_unique::<ServerStatus>();
        assert_unique::<ActorType>();
        assert_unique::<VolumePlacement>();
    }
}
