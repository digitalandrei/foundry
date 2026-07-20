// Audit-action presentation: the single source for how an audit
// `action` string renders (label + badge tone) and the filter options.
// Reused by the Audit Logs table and its filter so the vocabulary lives
// once (docs/FRONTEND_RULES.md § Structure & Reuse).

type BadgeVariant = "default" | "secondary" | "destructive" | "outline"

export interface AuditActionMeta {
  label: string
  variant: BadgeVariant
}

// Every action string emitted by the controller's audit::record sites.
const KNOWN: Record<string, AuditActionMeta> = {
  LOGIN: { label: "Login", variant: "secondary" },
  LOGOUT: { label: "Logout", variant: "outline" },
  SHELL_OPENED: { label: "Shell opened", variant: "secondary" },
  DEPLOYMENT_CREATED: { label: "Deployment created", variant: "default" },
  DEPLOYMENT_REPLACED: { label: "Deployment replaced", variant: "default" },
  DEPLOYMENT_DISMISSED: { label: "Deployment dismissed", variant: "destructive" },
  VOLUME_DELETED: { label: "Volume deleted", variant: "destructive" },
  VOLUME_CLEAN_REQUESTED: { label: "Volume clean requested", variant: "destructive" },
  AGENT_ENROLLED: { label: "Agent enrolled", variant: "default" },
  SERVER_CREATED: { label: "Server created", variant: "default" },
  ENROLLMENT_TOKEN_CREATED: { label: "Enrollment token minted", variant: "secondary" },
  INSTANCE_ONBOARDED: { label: "Instance onboarded", variant: "default" },
  INSTANCE_UPDATED: { label: "Instance updated", variant: "outline" },
  INSTANCE_DELETED: { label: "Instance deleted", variant: "destructive" },
}

/** Ordered options for the action filter Select (value = stored action). */
export const AUDIT_ACTION_OPTIONS: { value: string; label: string }[] = Object.entries(
  KNOWN,
).map(([value, meta]) => ({ value, label: meta.label }))

/** Operator-facing label + badge tone for an action. Unknown/future
 * actions degrade to a humanized label + neutral tone, so a new action
 * string shipped ahead of this map never breaks the table. */
export function auditActionMeta(action: string): AuditActionMeta {
  return KNOWN[action] ?? { label: humanize(action), variant: "outline" }
}

function humanize(action: string): string {
  const lower = action.toLowerCase().replace(/_/g, " ")
  return lower.charAt(0).toUpperCase() + lower.slice(1)
}
