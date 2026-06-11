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
  id: string
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
  index: number
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
  containers_running: number
}

export interface PortMapping {
  container_port: number
  host_port: number | null
  protocol: string
}

export interface ServerContainer {
  container_id: string
  name: string
  image: string
  state: string
  status: string
  managed: boolean
  ports: PortMapping[]
  reported_at: string
}

export interface HostMetrics {
  cpu_pct: number
  mem_used_mb: number
  mem_total_mb: number
  disk_used_gb: number
  disk_total_gb: number
  net_rx_bps: number
  net_tx_bps: number
}

export interface GpuMetrics {
  uuid: string
  util_pct: number
  mem_used_mb: number
  temperature_c: number
  power_w: number
}

export interface ContainerMetrics {
  container_id: string
  cpu_pct: number
  mem_used_mb: number
  mem_limit_mb: number
}

export interface MetricsSample {
  host: HostMetrics
  gpus: GpuMetrics[]
  containers: ContainerMetrics[]
}

export interface MetricsPoint {
  sampled_at: string
  sample: MetricsSample
}

export type PortKind = "HTTP" | "HTTPS" | "TCP" | "UDP"

export interface PortSpec {
  container_port: number
  kind: PortKind
  host_port?: number | null
}

export interface EnvSpec {
  key: string
  value: string
  is_secret: boolean
}

export interface VolumeSpec {
  volume_name: string
  container_path: string
  read_only: boolean
}

export interface CreateDeploymentRequest {
  slot_id: string
  registry_tag_id: string
  name?: string
  ports: PortSpec[]
  env: EnvSpec[]
  volumes: VolumeSpec[]
}

export interface DeploymentPort {
  container_port: number
  host_port: number
  protocol: string
  kind: PortKind
}

export interface DeploymentSummary {
  id: string
  name: string
  image_ref: string
  state: import("./states").DeploymentState
  error_message: string | null
  server_id: string
  server_name: string
  slot_id: string
  slot_name: string
  gpu_label: string
  created_by_name: string
  ports: DeploymentPort[]
  created_at: string
  started_at: string | null
}

export interface ServerVolume {
  id: string
  name: string
  path: string
  created_by_name: string
  attached_to: string[]
  created_at: string
}

/** Drag payload: containers-panel tag card → slot chip. */
export interface DragTagData {
  registryTagId: string
  imageName: string
  tagName: string
  sizeBytes: number | null
}

export interface DropSlotData {
  slotId: string
  slotName: string
  slotState: import("./states").SlotState
  serverId: string
  serverName: string
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
