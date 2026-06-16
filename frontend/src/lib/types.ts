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

export interface UpdateInstanceRequest {
  name: string
  base_url: string
  registry_url: string
  oauth_client_id: string
  /** Omit/empty to keep the stored secret. */
  oauth_client_secret?: string
  enabled: boolean
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
  /** Wildcard apps domain when HTTP/S publishing is enabled. */
  apps_domain: string | null
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

/** EXPOSE'd ports read from the image config (deploy-dialog prefill). */
export interface ExposedPort {
  container_port: number
  protocol: string
}

export interface ExposedPortsResponse {
  ports: ExposedPort[]
}

export type ServerStatus = "ONLINE" | "OFFLINE" | "DEGRADED"

export interface ExternalOccupant {
  name: string
  image: string
  /** Running (using the GPU) vs stopped/exited (device free, shown anyway). */
  running: boolean
}

export interface SlotSummary {
  id: string
  name: string
  slot_type: "FULL_GPU" | "MIG_SLOT"
  mig_profile: string | null
  capacity_mb: number | null
  state: import("./states").SlotState
  /** A non-Foundry container running on this slot's GPU/MIG device. */
  external: ExternalOccupant | null
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
  /** HTTP/S publishing readiness: true → ready, false → not ready (see
   * nginx_status), null → unknown / no recent snapshot. */
  app_publishing_ready: boolean | null
  /** Granular nginx status: READY | NGINX_MISSING | NGINX_OUTDATED |
   * NGINX_INACTIVE | NOT_CONFIGURED | TLS_MISSING, or null (pre-0.16
   * agent / no snapshot). */
  nginx_status: string | null
  /** Docker daemon liveness: true → active, false → down (deploys
   * blocked), null → unknown / no snapshot yet. */
  docker_ok: boolean | null
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
  load_avg_1m: number
  cpu_cores: number
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
  cpu_cores: number
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
  /** HTTP/HTTPS: the published app hostname (open `https://{hostname}`). */
  hostname: string | null
}

export interface DeploymentSummary {
  id: string
  name: string
  image_ref: string
  state: import("./states").DeploymentState
  /** Live progress while a DEPLOY task runs (`pulling: 3/7 layers …`). */
  status_detail: string | null
  /** Docker container id — joins the telemetry sample's containers. */
  container_id: string | null
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

export interface DeploymentMount {
  /** null when the backing persistent volume was deleted later. */
  volume_name: string | null
  host_path: string
  container_path: string
  read_only: boolean
}

export interface DeploymentEnvKey {
  key: string
  is_secret: boolean
}

/** GET /api/deployments/{id} — summary fields are flattened in. */
export interface DeploymentDetail extends DeploymentSummary {
  mounts: DeploymentMount[]
  env: DeploymentEnvKey[]
}

/** GET /api/deployments/{id}/logs — bounded recent stdout+stderr. */
export interface DeploymentLogsView {
  /** Merged stdout+stderr (oldest→newest), capped to the response budget. */
  content: string
  /** Timestamp of the newest captured line (null → nothing captured yet). */
  collected_at: string | null
  available: boolean
}

/** GET /api/metrics/latest — newest telemetry sample per server. */
export interface LatestServerMetrics {
  server_id: string
  sampled_at: string
  sample: MetricsSample
}

export interface LatestMetricsResponse {
  servers: LatestServerMetrics[]
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

/** One row of the append-only audit trail (GET /api/audit). Mirrors
 * shared::dto::AuditLogEntry. `actor_name` is resolved server-side;
 * null for agent/system actors. `detail` is the raw JSON recorded with
 * the action. */
export interface AuditLogEntry {
  id: string
  actor_type: string
  actor_id: string | null
  actor_name: string | null
  action: string
  subject_type: string | null
  subject_id: string | null
  detail: Record<string, unknown> | null
  ip_address: string | null
  created_at: string
}

/** Cursor-paginated audit page. `next_cursor` → pass as `?before=` for
 * the next (older) page; null at the end. Mirrors shared::dto::AuditPage. */
export interface AuditPage {
  entries: AuditLogEntry[]
  next_cursor: string | null
}

/** One freshly-discovered image tag (GET /api/registry/updates). Mirrors
 * shared::dto::RegistryNewTag. */
export interface RegistryNewTag {
  id: string
  tag_name: string
  repo_path: string
  project_id: string
}

export interface RegistryUpdates {
  new_tags: RegistryNewTag[]
}
