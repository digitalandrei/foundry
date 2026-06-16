//! Typed UUID newtypes — one per identified table (`docs/DATABASE.md`).
//!
//! All Foundry primary keys are UUIDv7 (time-ordered), stored as
//! BINARY(16). Newtypes keep a `DeploymentId` from ever being passed
//! where a `SlotId` is expected.

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub uuid::Uuid);

        impl $name {
            /// A fresh time-ordered (v7) id.
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            pub fn as_uuid(&self) -> &uuid::Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<uuid::Uuid> for $name {
            fn from(u: uuid::Uuid) -> Self {
                Self(u)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(UserId);
id_type!(GitlabInstanceId);
id_type!(GitlabAccountId);
id_type!(GitlabProjectId);
id_type!(RegistryRepositoryId);
id_type!(RegistryTagId);
id_type!(ServerId);
id_type!(ServerAgentId);
id_type!(GpuId);
id_type!(SlotId);
id_type!(GpuGroupId);
id_type!(EnrollmentTokenId);
id_type!(DeploymentId);
id_type!(TaskId);
id_type!(ServerVolumeId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_v7_and_serde_transparent() {
        let id = DeploymentId::new();
        assert_eq!(id.as_uuid().get_version_num(), 7);
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, format!("\"{id}\""));
        let back: DeploymentId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
    }

    #[test]
    fn v7_ids_are_time_ordered() {
        let a = TaskId::new();
        let b = TaskId::new();
        assert!(a < b);
    }
}
