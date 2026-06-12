# Foundry UI Design

The visual and interaction contract for the frontend, derived from the
approved dashboard mockup (dark mode). Code-level rules live in
`FRONTEND_RULES.md`. Keep slot-state semantics in sync with
`ARCHITECTURE.md` § Slot states.

> Mockup file: `docs/assets/dashboard-mockup-dark.png` — **placeholder**:
> the image was shared in conversation; drop the PNG at this path when
> convenient. The description below is authoritative until then.

## Pages

Top navigation: **Dashboard · Deployments · Servers · Audit Logs · Settings**
(+ notification bell, help, user menu with avatar at top right).
GitLab browsing (the spec's "Projects" and "Registry" pages) lives in the
persistent left sidebar rather than separate pages.

## Dashboard Layout (the core screen)

Three regions:

### Scroll model (operator feedback, 2026-06-11; refined 0.10.0)

The dashboard is app-like: on wide screens the page itself never
scrolls — every box scrolls inside itself (containers tree, servers
grid, deployments table each in their own card; System Status stays
put). Below the `lg` breakpoint the boxes stack vertically and the
page scrolls normally — content is never trapped.

### 1. Left sidebar — "Available Containers (from GitLab)"

- GitLab-branded section: nav entries **Projects / Groups / Registry /
  Favorites**; header shows the source registry (e.g. `registry.gitlab.com`;
  with multi-instance, the instance is selectable).
- Filters: container search box, project dropdown ("All Projects"), tag
  dropdown ("All Tags"), "Show only mine" checkbox.
- Search semantics (operator feedback, 2026-06-11): one query filters
  project paths AND — inside expanded projects — repository paths +
  tag names. Tags load lazily, so collapsed projects' tags aren't
  searched (the empty-result hint says so). Long tag lists show the
  first 8 with a "Show N more" expander; a filtered view shows all
  matches.
- Collapsible project groups (`namespace / project` with item count badge),
  each listing **container cards**: package icon, image name, version tag
  (e.g. `v1.7.0`) and size (e.g. `2.8 GB`). These cards are the **drag
  sources**.
- Bottom: System Status card (Servers online, GPUs total, Containers
  running) and app version (`Foundry vX.Y.Z`).
- Hint line: "Drag a container to a GPU slot to deploy".

### 2. Main panel — "Servers & GPU Slots (NVIDIA MIG)"

- Header with state legend: **Free / Running / Reserved / Offline**.
- One row per server: status dot, name (`gpu-server-01`), IP, OS, GPU model,
  `MIG Mode: Enabled|Disabled`, totals (`8 GPUs | 64 MIG Slices`).
- GPUs render as **cells that split the full row width** (0.10.0,
  operator feedback: chips were small with space to spare) —
  `repeat(auto-fit, minmax(280px, 1fr))`, so 2 GPUs split 50/50.
  - GPU cell header: index, model, MIG badge, and **live silicon
    telemetry** (util % · mem GB · temp °C · power W) from
    `GET /api/metrics/latest`. NVML cannot attribute per MIG slice, so
    GPU-level stats live here, container stats on the chips.
  - Slot chips stretch to fill their GPU cell (a full-GPU slot fills
    it entirely). Chip anatomy: slot name `g:i` (display only —
    identity is the UUID) + capacity/MIG profile + state label, then
    when occupied the workload name and a small live-usage line
    (`CPU 12% · MEM 3.4/16 GB`, or the in-flight progress text while
    deploying — `pulling: 3/7 layers · 410 / 1208 MB`).
  - **Clicking an occupied chip opens the slot detail dialog**
    (`slot-detail-dialog.tsx`, backed by `GET /api/deployments/{id}`):
    state + live progress/error, image, usage, ports incl. clickable
    app URLs, **mounts** (volume name, container path, ro, host path),
    env *names* (secrets shown as `•••` — values never leave the
    server), uptime/creator. Presentational only — lifecycle actions
    stay on the Deployments page.
- **Drag interaction**: dragging a container card over a valid `FREE` slot
  shows a dashed highlighted drop target (mockup: dashed green outline with
  a floating card ghost showing image + version + size). Dropping opens the
  deployment config dialog; dropping on an occupied slot opens the
  replacement confirmation (see `ARCHITECTURE.md` § Replacement workflow).
- Offline servers render gray with hollow status dot; their slots are inert.

### 3. Bottom panel — "Deployments"

Table with running-count badge and "View All" link to the Deployments
page. Shows everything alive on the fleet — including **in-flight
deploys with live progress text** under the status label and FAILED
rows with their error (REPLACED history stays on the Deployments
page). Columns: **Name** · **Image** · **Server** · **GPU / Slice** ·
**Status** (colored; progress/error sub-line) · **Uptime**. Actions
live on the Deployments page.

## Slot State Colors

Single source of truth (the state→color map in `frontend/src/lib/`):

| State | Color token |
|---|---|
| FREE | green |
| RUNNING | blue |
| RESERVED / DEPLOYING / STOPPING | yellow |
| FAILED | red |
| OFFLINE | gray |

Status text in tables uses the same tokens (e.g. "Running" in blue/green per
mockup — final token mapping fixed when the palette lands in Phase 8).

## Theming: Dark + Light

- **Dark mode is the default** (matches the mockup: near-black background,
  elevated card surfaces, subtle borders, high-contrast white text, vivid
  state accents).
- **Light mode is required**, switchable from the user menu/Settings and
  persisted (localStorage + `prefers-color-scheme` default).
- Implementation: shadcn/Tailwind CSS variables — every color in components
  is a semantic token (`--background`, `--card`, `--primary`,
  `--destructive`, plus Foundry state tokens `--slot-free`, `--slot-running`,
  `--slot-reserved`, `--slot-offline`, `--slot-failed`) defined once for
  `:root` (light) and `.dark`. **No hardcoded palette colors in
  components** — that is what makes light mode a token-swap instead of a
  rewrite.
- Every new screen is verified in both themes before it is "done"
  (`FRONTEND_RULES.md` checklist).

## Other Screens (initial sketches; refined in Phase 8)

- **Deployments** (live since 0.7.0): table with image/server/slot/
  ports/state (deployment color map)/uptime/creator and per-state
  actions (stop, start, remove — volumes survive removal, said in the
  tooltip); failed rows show the error in a tooltip. Detail view with
  lifecycle timeline (`deployment_events`) and logs comes with
  Phase 7/8. The deploy dialog (drag-drop) collects name, multi-port
  rows (TCP/UDP now), env rows with secret toggle, and persistent
  volume mounts with reuse suggestions.
- **Servers** (live since 0.2.0): table with status dot (same color
  tokens), hostname/OS/agent version/last heartbeat; admin "Add
  server" dialog → names the server, shows the one-time registration
  command + binary download hint; "New token" re-mint per server.
  Since 0.5.0: "Details" (and server names on the dashboard) navigate
  to the dedicated **`/servers/{id}` page**: header (status, hostname,
  OS/Docker/driver/agent versions), four host metric cards with
  sparklines (CPU %, memory, disk /, network rates), one card per GPU
  (utilization sparkline + current mem/temp/power), and the containers
  table with per-container CPU/mem and **port mappings**
  (`host→container/proto`). Series: 30s samples, 24h retention.
  **Graph scale rule (operator, 0.6.0):** percentage graphs are always
  pinned 0–100; capacity-bound graphs (memory, disk, GPU memory) are
  pinned 0–capacity; only truly unbounded series (network rates,
  power) auto-scale. Each GPU card groups four graphs: usage, memory,
  temperature (0–100 °C), power.
- **Audit Logs**: filterable audit table (actor, action, subject, time).
- **Settings**: GitLab instances (admin), enrollment tokens, theme, profile.
- **Help**: `/help/gitlab-oauth` — GitLab OAuth app setup guide (steps,
  required scopes with rationale, leave-unchecked list). Reached from
  the top-nav help icon and the Settings onboarding form. Content must
  stay in sync with `GITLAB-INTEGRATION.md` and the controller's scope
  list.
