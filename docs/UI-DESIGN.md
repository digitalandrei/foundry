# Foundry UI Design

The visual and interaction contract for the frontend, derived from the
approved dashboard mockup (dark mode). Code-level rules live in
`FRONTEND_RULES.md`. Keep slot-state semantics in sync with
`ARCHITECTURE.md` § Slot states.

> Mockup file: `docs/assets/dashboard-mockup-dark.png` — **placeholder**:
> the image was shared in conversation; drop the PNG at this path when
> convenient. The description below is authoritative until then.

## Pages

Top navigation: **Dashboard · Deployments · Servers · Storage · Telemetry · Audit Logs · Settings**
(+ notification bell, help, user menu with avatar at top right).
GitLab browsing (the spec's "Projects" and "Registry" pages) lives in the
persistent left sidebar rather than separate pages.

Responsive header: the inline nav row only fits comfortably at `lg` and
up. Below `lg` it collapses behind a hamburger menu (same items, same
active styling) so the header never exceeds a phone viewport — the brand,
help, theme, and user controls stay on the bar. The app shell is
`overflow-x-clip` so no wide child (table, grid, console) can drag the
whole page sideways; wide content scrolls inside its own box.

Routes are lazy-loaded behind one named, live-region loading state so the eleven
pages do not inflate the initial application chunk. Any occupied GPU/group
surface that opens a deployment is focusable, has an accessible name, and
activates with Enter or Space as well as pointer input; its drop-only free
counterpart stays out of the tab order.

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
  (e.g. `v1.7.0`) and size (e.g. `2.8 GB`). These cards deploy two ways:
  **tap** one to open the slot picker, or **drag** it onto a slot.
- **New-image awareness** (0.27.0): while you're in the app the SPA polls
  for newly-pushed tags across your available repos (cheap name-only
  sync); a toast announces them, a dot marks the affected **project**
  while collapsed and a `new` badge marks the **repo**, clearing once you
  open the project and collapse it again. An expanded project's tree
  refreshes in place so the new tag shows up where it belongs.
- Bottom: System Status card (Servers online, GPUs total, Containers
  running) and app version (`Foundry vX.Y.Z`).
- Hint line: "Tap a container to pick a slot, or drag it onto one".

### 2. Main panel — "Servers & GPU Slots"

The main panel fills the full height of the dashboard (0.20.0: the
bottom Deployments box was removed — deployments live on their own page
now; the grid stretches and scrolls inside itself). MIG is shown
**per GPU** (a `MIG` / `No MIG` marker on each GPU cell), not in the
panel title.

- Header with state legend: **Free / Running / Reserved / Offline**.
- One row per server: status dot, name (`gpu-server-01`), IP, OS, plus
  per-server health badges — **`docker: active`** (green) or **`Docker
  stopped — deploys blocked`** (red), and the nginx/HTTP-S badge. On the
  right, just left of the **`N GPUs`** count, the **host readout**
  (`GET /api/metrics/latest`): `cpu <load> / <cores> · mem <used> / <total>
  GB · disk <used> / <total> GB` — CPU is per-server (1-min load average
  over logical cores), distinct from the per-GPU silicon stats; disk is the
  root filesystem; hidden while the server is OFFLINE (the last sample would
  be stale). When a usage threshold trips, an **amber warning badge** joins
  the docker/nginx cluster and the matching readout value tints amber —
  `disk ≥90%`, `mem ≥90%`, or `cpu ≥1.0×` (1-min load ≥ cores); quiet
  otherwise. A
  server whose Docker daemon is down accepts no deploys (drop targets
  inert; the controller also rejects at create).
- GPUs render as **cells that split the full row width** (0.10.0,
  operator feedback: chips were small with space to spare) —
  `repeat(auto-fit, minmax(280px, 1fr))`, so 2 GPUs split 50/50.
  - GPU cell header: index, model, MIG badge, and **live silicon
    telemetry** (util % · mem GB · temp °C · power W) from
    `GET /api/metrics/latest`. NVML cannot attribute per MIG slice, so
    GPU-level stats live here, container stats on the chips.
  - Slot chips stretch to fill their GPU cell (a full-GPU slot fills
    it entirely). Chip anatomy (0.23/0.24): the **first line** carries
    `SLOT g:i` (display only — identity is the UUID), the occupant's name,
    its **run-state inline** (`running` / `stopped` / `deploying` …), and
    on the right the slot state label (Free / Locked / Deploying /
    Running / Freeing / …) + capacity/MIG profile. A second line appears
    only when there's live detail: the container's **own** stats when
    running — `<cpu cores> / <allotted cores> · <used> / <limit> GB`
    (RAM, not VRAM; VRAM lives on the GPU header) — or the in-flight
    progress text while deploying (`pulling: 3/7 layers …`). The
    server row shows the hostname only when it differs from the name.
  - **Clicking an occupied chip opens the dedicated deployment page**
    (`/deployments/{id}`, `deployment-detail.tsx`, backed by
    `GET /api/deployments/{id}` + `…/logs`). It's a **full-screen, three-
    region layout** (0.21.0). A **header bar** carries the name, state,
    server/slot, and the **same state-gated lifecycle action buttons as
    the Deployments list** (stop · re-deploy · delete · dismiss; 0.26.0) to
    its right — actions run through the shared mutations, so one press is
    reflected on the list, the detail view, and the slot grid at once.
    Then **(1) Details** on top — state + live
    progress/error, image, usage, ports incl. clickable app URLs,
    **mounts** (volume name, container path, ro, host path), env *names*
    (secrets `•••` — values never leave the server), uptime/creator;
    **(2) Console** (merged stdout+stderr; Follow, Copy on the title line)
    and **(3) Shell** — a live **xterm.js terminal** (0.22.0) — side by
    side below, each expandable to full width. On phones the three boxes
    stack one-per-viewport (swipe details → console → shell). The shell
    **connects only on an explicit Start click** (a deliberate action; it
    `docker exec`s bash→sh via the agent-dialed reverse-WS tunnel),
    Disconnects from the title line, and Reconnects after a session ends.
    Each box's actions sit on its title line, with Expand last. Lifecycle
    actions stay on the Deployments page.
  - **GPU groups (overlay model).** Member GPUs keep their own cells and
    stay individually deployable when no group job runs; each cell header
    carries small **`grp <names>` membership chips** (a GPU may be in
    several groups — overlap is shown, with a tooltip spelling the names).
    Groups are a **separate deploy affordance rendered like a GPU cell**: a
    per-server **Groups strip** below the GPU cells shows each group as a box
    headed `GROUP SLOT <name>` with its **member GPUs as `GPU n` badges**
    beside the name (wrapping to more lines when narrow) and the combined
    `N GPUs · <VRAM> GB` on the right. Below the header sit the group's
    **`SLOT 1..N` deploy positions** (N = the group's `max_occupants`: 1
    single-use, up to 4 multi-use), rendered exactly like a GPU's slots. A
    free slot is a drop + tap target that deploys **one container across
    every member GPU**; an occupied slot clicks through to its deployment.
    Groups **never replace** (a deploy needs every member free). Deployability
    is never recomputed client-side — the `deployable`/`busy_reason` from the
    API (which names the holder for an overlapping group) are authoritative; a
    free slot on a blocked group reads `Busy` with the reason on hover.
  - **Multi-use slots (soft sharing).** A slot's `max_occupants` (1 =
    single-use; 2…4 = multi-use) is operator config. A slot expands into
    **exactly `max_occupants` `SLOT n` positions** (numbered 1-based per
    GPU); the i-th co-tenant fills the i-th position and the rest read `Free`
    and stay drop/tap targets — sharing never offers "replace". Sharing has
    **no VRAM isolation** (MIG is the isolated path).
- **Deploy interaction**: two equivalent paths into the same config dialog.
  **Drag** a container card over a valid `FREE` slot to show a dashed
  highlighted drop target (mockup: dashed green outline with a floating card
  ghost showing image + version + size); dropping opens the deployment
  config dialog, and dropping on an occupied slot opens the replacement
  confirmation (see `ARCHITECTURE.md` § Replacement workflow). **Tap** (the
  touch/keyboard path, primary on mobile) opens a **slot picker** dialog
  listing every server's GPUs, their `SLOT n` positions, **and groups** — a
  free position deploys, an occupied single-use slot replaces, a group slot
  deploys across its members, ineligible targets show disabled with the
  reason. Both paths honor the same eligibility from one
  source (`lib/slots.ts`); groups use the API's `deployable`/`busy_reason`. The dnd sensors
  keep a tap from registering as a drag and let a vertical swipe still
  scroll the container list. The config dialog carries a **Memory limit**
  slider (32–256 GB; slide fully right or tick *Unlimited* for no Docker
  `--memory` cap — the default, so typical deploys are unconstrained).
  The cap is set only here, at deploy time.
- Offline servers render gray with hollow status dot; their slots are inert.

### 3. Deployments

There is no bottom Deployments box on the dashboard (removed in 0.20.0
so the slot grid can fill the panel). The **Deployments page** is the
full table — each **row clicks through to the deployment page**
(`/deployments/{id}`) for details + console, and a console icon does the
same. Rows carry the lifecycle actions. Destructive ones (Stop, Delete) open a
**confirmation modal** with a red **CONFIRM** button (`confirm-dialog.tsx`
/ `useConfirm` — never the native `window.confirm`); in-flight deploys
show live progress; FAILED rows show their error. `PUBLISH_FAILED` is a
distinct recoverable state with **Retry publishing**, Stop and Delete; its
healthy container and shell remain available. Deployment detail shows the
immutable digest, Docker health and an **Application traffic** card (24h
request/error/bytes/latency/status metrics plus recent request rows). The
primary image-declared app URL is preferred in compact slot surfaces. The
dashboard's **System Status** card keeps the running/online counts.

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
  rows (TCP/UDP now), env rows with secret toggle, and **persistent-storage
  source → container-destination mappings**. Each mount card starts in
  **Automatic root** mode (current deploy-name project + logical mount +
  SLOT/SERVER placement) and has a searchable **Existing root** alternative.
  The picker exposes every physically compatible root, not only the current
  project: `server → exact Slot/Group or Shared → Project → Mount`. A slot
  target sees that exact slot plus all shared roots; a group sees that exact
  group plus all shared roots. It never offers another slot/group/server.
  Each picker result displays and announces its owner, measured usage/quota,
  and an active/retained-reference summary; after selection, its source card
  lists active/retained and recent mapping details. Selecting it preserves its
  source identity and shows it as a Docker bind source; the operator can map
  it to any unique absolute container destination and choose RO/RW
  independently. A sharing warning is informative for non-purging reuse. A
  new purge-on-redeploy mapping is rejected while an unrelated active or
  retained deployment still references the root; an existing purge policy is
  called out because a later redeploy can clear shared files. During a
  replacement, the editable form and submit action remain unavailable until
  Foundry has loaded the predecessor's exact bindings; a load failure presents
  an explicit retry state rather than falling back to image defaults.
  Automatic placement controls remain available only in Automatic mode. SLOT
  follows the physical or GPU-group slot; SERVER follows the same deploy name
  across that server. Storage keys are `placement / deploy name / mount name`,
  where the deploy name is the name entered in this dialog. During replacement
  this name is displayed as fixed, not editable: it preserves the deployment's
  app URL and storage-project namespace. Replacement begins with its
  predecessor's exact mapping cards, then permits intentional compatible
  source/mapping changes. Opening the dialog inspects the image:
  EXPOSE ports and persistent mounts declared by Docker `VOLUME` or the
  Foundry volume-default label are prefilled once, remain editable, and do
  not overwrite input the operator has already changed. The optional Foundry
  apps label also supplies primary web port, health path, request ceiling and
  proxy timeout; the UI keeps one deterministic primary URL.
- **Storage** (placement model 0.63.0): select a local server, then use a
  responsive dual-pane, MC-inspired browser over its SLOT and SERVER roots.
  GitLab projects are not a storage dimension. Every selector is search-first:
  the server selector matches node name/hostname, and the root selector is
  grouped as **Shared / Slot / Group → Project → Mount** while matching terms
  across server, placement, project/deployment name, mount, attachment, and
  their displayed aliases. It supports tokenized AND search plus keyboard
  Arrow/Home/End/Enter navigation. The policy table and pane selector show the
  same `server / placement / project / mount` breadcrumb for every root; each
  pane selects a root and navigates independently. Double-click/Enter opens
  directories or
  bounded UTF-8 text in a monospaced editor; toolbar actions create, rename,
  copy to the other pane, move, download, upload and type-to-confirm delete.
  Native desktop files drop into either pane/directory; dragging entries
  between panes copies server-side. Below it, the policy table still exposes
  placement, creator and active attachments. Creator/admin
  **Clean** irreversibly wipes contents but retains identity; **Delete** wipes
  both. Mounted volumes disable those whole-volume actions. Used/quota bytes
  and over/near-quota warnings come from agent measurement. Creator/admin can
  set/remove an advisory quota. Desktop upload resumes after reconnect by
  reselecting the same local file (stable upload identity + server offset).
  Opening an occupied slot/deployment shows the same browser narrowed to the
  exact persistent roots mounted by that deployment.
- **Servers** (live since 0.2.0): table with status dot (same color
  tokens), hostname/OS/agent version/last heartbeat; admin "Add
  server" dialog → names the server, shows the one-time registration
  command + binary download hint; "New token" re-mint per server.
  Since 0.5.0: "Details" (and server names on the dashboard) navigate
  to the dedicated **`/servers/{id}` page**: header (status, hostname,
  OS/Docker/driver/agent versions), five host metric cards with
  sparklines (CPU %, **Load** = 1-min load average / logical cores,
  memory used/max, disk /, network rates), one card per GPU
  (utilization sparkline + current mem/temp/power), and the containers
  table with per-container **Load** (cores used / cores) + memory
  (used/max) and **port mappings** (`host→container/proto`). Series:
  30s samples, 24h retention.
  A **Host readiness** card lists live Docker, storage-write, capability,
  nginx-config, wildcard-certificate and setup-revision checks with exact
  details, plus persistent filesystem used/free capacity. Admin actions run
  diagnostics or trigger **Upgrade & repair**; upgrade is disabled with a
  one-time bootstrap hint for agents older than 0.59.0.
  **Graph scale rule (operator, 0.6.0):** percentage graphs are always
  pinned 0–100; capacity-bound graphs (memory, disk, GPU memory) are
  pinned 0–capacity; only truly unbounded series (network rates,
  power) auto-scale. Each GPU card groups four graphs: usage, memory,
  temperature (0–100 °C), power. **Admin-only — GPU groups & slot
  sharing** (hidden for non-admins): below the GPU cards, a section to
  manage **groups** (list with member GPUs + combined VRAM +
  deployable/busy state + delete, disabled with the reason while a deploy
  is live; "New group" = name + a multi-select of this server's full-GPU,
  MIG-disabled cards, 2…all, overlap allowed with current memberships shown
  inline) and **per-slot use-mode** (single-use / multi-use with a
  max-occupant 2…4, behind a loud no-VRAM-isolation caveat; lowering the
  cap below current occupants stops new deploys but does not evict).
- **Telemetry**: fleet-wide deep-dive — every enrolled server's host +
  per-GPU graphs (usage/memory/temperature/power) plus per-MIG-slice
  memory, on one scrollable page (24h series). The dashboard keeps the
  at-a-glance summary; this is where the full history lives.
- **Audit Logs**: filterable audit table (actor, action, subject, time).
- **Settings**: GitLab instances (admin), enrollment tokens, theme, profile.
- **Help**: `/help/gitlab-oauth` — GitLab OAuth app setup guide (steps,
  required scopes with rationale, leave-unchecked list). Reached from
  the top-nav help icon and the Settings onboarding form. Content must
  stay in sync with `GITLAB-INTEGRATION.md` and the controller's scope
  list.
