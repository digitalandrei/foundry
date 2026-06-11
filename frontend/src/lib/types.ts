// Mirror of shared/src/dto — the wire contract. Field names match the
// Rust serde output exactly; change both sides in the same commit.

export interface ErrorEnvelope {
  error: { code: string; message: string }
}

export interface InstancePublic {
  id: string
  name: string
}

export interface InstanceAdmin {
  id: string
  name: string
  base_url: string
  registry_url: string
  oauth_client_id: string
  enabled: boolean
}

export interface CreateInstanceRequest {
  name: string
  base_url: string
  registry_url: string
  oauth_client_id: string
  oauth_client_secret: string
}

export interface GitlabAccountSummary {
  instance_id: string
  instance_name: string
  username: string
}

export interface MeResponse {
  id: string
  display_name: string
  email: string | null
  avatar_url: string | null
  is_admin: boolean
  accounts: GitlabAccountSummary[]
}

export interface ProjectSummary {
  id: string
  instance_id: string
  gitlab_project_id: number
  name: string
  path_with_namespace: string
  avatar_url: string | null
}

export interface RegistryTag {
  name: string
  size_bytes: number | null
  pushed_at: string | null
}

export interface RegistryRepository {
  id: string
  path: string
  tags: RegistryTag[]
}

export interface RegistryBrowseResponse {
  repositories: RegistryRepository[]
}

export type ServerStatus = "ONLINE" | "OFFLINE" | "DEGRADED"

export interface SlotSummary {
  id: string
  name: string
  slot_type: "FULL_GPU" | "MIG_SLOT"
  mig_profile: string | null
  capacity_mb: number | null
  state: import("./states").SlotState
}

export interface GpuSummary {
  id: string
  gpu_uuid: string
  model: string | null
  memory_mb: number | null
  mig_enabled: boolean
  slots: SlotSummary[]
}

export interface ServerSummary {
  id: string
  name: string
  hostname: string | null
  status: ServerStatus
  last_heartbeat_at: string | null
  agent_version: string | null
  os_version: string | null
  enrolled: boolean
  gpus: GpuSummary[]
}

export interface ServerContainer {
  container_id: string
  name: string
  image: string
  state: string
  status: string
  managed: boolean
  reported_at: string
}

export interface ServerDetail {
  server: ServerSummary
  docker_version: string | null
  nvidia_driver_version: string | null
  gpus: GpuSummary[]
  containers: ServerContainer[]
}

export interface EnrollmentTokenResponse {
  server: ServerSummary
  token: string
  command: string
  expires_at: string
}
