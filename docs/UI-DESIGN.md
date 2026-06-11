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

### 1. Left sidebar — "Available Containers (from GitLab)"

- GitLab-branded section: nav entries **Projects / Groups / Registry /
  Favorites**; header shows the source registry (e.g. `registry.gitlab.com`;
  with multi-instance, the instance is selectable).
- Filters: container search box, project dropdown ("All Projects"), tag
  dropdown ("All Tags"), "Show only mine" checkbox.
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
- Per GPU (`GPU 0 (A100 80GB)`): a horizontal strip of **slot chips**.
  - Chip anatomy: slot name `g:i` (GPU index : slice index — display only,
    identity is the UUID), MIG profile (`1g.10gb`, `2g.20gb`, `3g.40gb`,
    `7g.80gb`), and when occupied the workload name + version
    (e.g. `1:1 · comfyui v0.3.29 · 1g.10gb`).
  - Non-MIG GPUs show full-GPU cards (`GPU 0 · 80 GB · (No MIG)`).
- **Drag interaction**: dragging a container card over a valid `FREE` slot
  shows a dashed highlighted drop target (mockup: dashed green outline with
  a floating card ghost showing image + version + size). Dropping opens the
  deployment config dialog; dropping on an occupied slot opens the
  replacement confirmation (see `ARCHITECTURE.md` § Replacement workflow).
- Offline servers render gray with hollow status dot; their slots are inert.

### 3. Bottom panel — "Running Deployments"

Table with count badge and "View All" link to the Deployments page.
Columns: **Name** (generated, e.g. `comfyui-7f9d2`) · **Image**
(`namespace/project`) · **Version** · **Server** · **GPU / Slice**
(`GPU 1 / 1:1 (1g.10gb)`) · **Status** (colored) · **Uptime** · **Actions**
(console/logs, metrics, delete).

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

- **Deployments**: full filterable table (superset of the dashboard panel) +
  deployment detail with lifecycle timeline (`deployment_events`) and logs.
- **Servers**: enrollment (token generation), server detail with GPU/MIG
  inventory and agent health.
- **Audit Logs**: filterable audit table (actor, action, subject, time).
- **Settings**: GitLab instances (admin), enrollment tokens, theme, profile.
- **Help**: `/help/gitlab-oauth` — GitLab OAuth app setup guide (steps,
  required scopes with rationale, leave-unchecked list). Reached from
  the top-nav help icon and the Settings onboarding form. Content must
  stay in sync with `GITLAB-INTEGRATION.md` and the controller's scope
  list.
