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

export interface ServerSummary {
  id: string
  name: string
  hostname: string | null
  status: ServerStatus
  last_heartbeat_at: string | null
  agent_version: string | null
  os_version: string | null
  enrolled: boolean
}

export interface EnrollmentTokenResponse {
  server: ServerSummary
  token: string
  command: string
  expires_at: string
}
