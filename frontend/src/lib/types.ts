// Mirror of shared/src/dto — the wire contract. Field names match the
// Rust serde output exactly; change both sides in the same commit.

export interface ErrorEnvelope {
  error: { code: string; message: string; details?: unknown }
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
  /** NVML MIG device UUID for a MIG slot, else null. Joins to per-slice
   * telemetry (`MigMetrics.uuid`). */
  mig_uuid: string | null
  mig_profile: string | null
  capacity_mb: number | null
  state: import("./states").SlotState
  /** Concurrency cap (multi-use). 1 = single-use; >1 = soft sharing with
   * no VRAM isolation. The grid shows occupancy as `k / max_occupants`. */
  max_occupants: number
  /** A non-Foundry container running on this slot's GPU/MIG device. */
  external: ExternalOccupant | null
}

/** A GPU's membership in a group (overlap allowed → may be several). */
export interface GpuGroupRef {
  id: string
  name: string
}

export interface GpuSummary {
  id: string
  gpu_uuid: string
  index: number
  model: string | null
  memory_mb: number | null
  mig_enabled: boolean
  slots: SlotSummary[]
  /** Groups this GPU belongs to — rendered as `grp A, B` chips. */
  groups: GpuGroupRef[]
}

/** Cap on a slot's max_occupants (multi-use). Operator decision: 1…4. */
export const MAX_OCCUPANTS_MIN = 1
export const MAX_OCCUPANTS_MAX = 4

/** GET /api/servers/{id}/gpu-groups — one container across N whole GPUs. */
export interface GpuGroup {
  id: string
  server_id: string
  name: string
  gpu_ids: string[]
  combined_vram_mb: number
  /** Group use-mode: 1 = single-use (one exclusive container across the
   * GPUs); >1 = multi-use (shared by up to N, no VRAM isolation). 1–4. */
  max_occupants: number
  /** Active deployments on this group now (`k` in `k / max_occupants`). */
  occupants: number
  /** Deployable iff below its cap and every member is online, eligible,
   * and free of non-group holders; else `busy_reason` names the blocker. */
  deployable: boolean
  busy_reason: string | null
  created_by_name: string
  created_at: string
}

export interface CreateGpuGroupRequest {
  name: string
  /** 2…all eligible (FULL, MIG-disabled) GPUs on the server. */
  gpu_ids: string[]
}

/** PATCH /api/slots/{id} — admin sets a slot's concurrency cap. */
export interface SetSlotUseModeRequest {
  max_occupants: number
}

/** PATCH /api/gpu-groups/{id} — admin sets a group's concurrency cap. */
export interface SetGroupUseModeRequest {
  max_occupants: number
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

export interface ContainerMount {
  source: string
  destination: string
  read_only: boolean
  mount_type: string
}

export interface ServerContainer {
  container_id: string
  name: string
  image: string
  state: string
  status: string
  managed: boolean
  ports: PortMapping[]
  mounts: ContainerMount[]
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

/** Per-MIG-instance memory (memory only — NVML reports no per-slice util).
 * Joins to a slot by `uuid === SlotSummary.mig_uuid`. */
export interface MigMetrics {
  uuid: string
  mem_used_mb: number
  mem_total_mb: number
}

export interface MetricsSample {
  host: HostMetrics
  gpus: GpuMetrics[]
  migs: MigMetrics[]
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

/** Where a deployment lands — exactly one of a slot or a group. */
export type DeployTarget =
  | { type: "slot"; slot_id: string }
  | { type: "group"; gpu_group_id: string }

export interface CreateDeploymentRequest {
  target: DeployTarget
  registry_tag_id: string
  name?: string
  ports: PortSpec[]
  env: EnvSpec[]
  volumes: VolumeSpec[]
  /** Docker memory cap in MB (deploy slider). Omit → unlimited. */
  mem_limit_mb?: number
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
  /** Denormalised primary (first/only) member slot. */
  slot_id: string
  slot_name: string
  /** Every member slot this deployment occupies (1 individual, N group);
   * the grid folds the occupant across all of these. */
  slot_ids: string[]
  /** Set for a group deploy (null = single-GPU); name drives the strip. */
  gpu_group_id: string | null
  group_name: string | null
  gpu_label: string
  created_by_name: string
  ports: DeploymentPort[]
  created_at: string
  started_at: string | null
  /** True when this wraps an adopted (externally-created) container —
   * the UI badges it and double-confirms destructive actions. */
  adopted: boolean
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
  kind: "slot"
  slotId: string
  /** Position label shown to the user (1-based within the GPU). */
  slotName: string
  slotState: import("./states").SlotState
  serverId: string
  serverName: string
  /** True → this drop replaces the running occupant (single-use slot);
   * false → a fresh deploy onto free capacity. Set explicitly because a
   * multi-use slot's `slotState` is RUNNING while it still has room. */
  replace: boolean
}

/** Drop payload for a GPU-group strip entry (one container : N GPUs). The
 * `kind` discriminator lets the dashboard tell a group drop from a slot. */
export interface DropGroupData {
  kind: "group"
  groupId: string
  groupName: string
  serverId: string
  serverName: string
  memberCount: number
  vramMb: number
}

export type DropData = DropSlotData | DropGroupData

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

/** Reusable fleet enrollment key (POST /api/fleet-tokens). Not bound to a
 * server — agents auto-enroll under their own hostname. */
export interface FleetTokenResponse {
  token: string
  command: string
  expires_at: string
  max_uses: number | null
}

/** A live fleet key in the management list (GET /api/fleet-tokens). The raw
 * token is never returned again — only this metadata. Many may coexist. */
export interface FleetTokenSummary {
  id: string
  created_by_name: string
  created_at: string
  expires_at: string
  max_uses: number | null
  uses: number
  expired: boolean
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
