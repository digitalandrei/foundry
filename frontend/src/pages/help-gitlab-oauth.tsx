import { CheckIcon, XIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"

// Keep in sync with the controller's requested scopes
// (controller/src/gitlab/oauth.rs SCOPES) and
// docs/GITLAB-INTEGRATION.md — the drift rule applies here too.
const REQUIRED_SCOPES = [
  {
    scope: "openid",
    purpose:
      "The sign-in itself: authenticates you to Foundry via OpenID Connect when you click “Sign in with GitLab”.",
  },
  {
    scope: "profile",
    purpose: "Read-only access to your name and avatar, shown in the Foundry user menu.",
  },
  {
    scope: "email",
    purpose:
      "Read-only access to your primary email — used to display who you are and to map operator (admin) rights.",
  },
  {
    scope: "read_api",
    purpose:
      "Read-only REST API access: lists the projects your account can see and browses their registry repositories and tags through the registry API. This is how your GitLab permissions become your Foundry permissions — Foundry keeps no permission system of its own.",
  },
  {
    scope: "read_registry",
    purpose:
      "Authorizes the container registry service itself (the JWT token exchange used for image pulls). read_api only covers the REST API — without read_registry, deployments would fail when the GPU server tries to pull the image with the short-lived credential Foundry mints.",
  },
] as const

const NOT_NEEDED = [
  { scope: "api", reason: "full write access — Foundry never writes to GitLab" },
  { scope: "write_registry", reason: "Foundry only pulls images, never pushes" },
  { scope: "read_repository / write_repository", reason: "Foundry never touches source code" },
  { scope: "read_user", reason: "already covered by openid + read_api" },
  {
    scope: "create_runner / manage_runner / k8s_proxy",
    reason: "CI runners and Kubernetes are out of Foundry's scope",
  },
  {
    scope: "observability / virtual registry / ai_features / service ping",
    reason: "unrelated to container deployment",
  },
  {
    scope: "sudo / admin_mode",
    reason: "never grant these to a third-party application",
  },
] as const

/** Help: connecting a GitLab instance (linked from Settings and the
 * top-nav help icon). Static content — the scope contract lives in
 * the controller; this page explains it. */
export function HelpGitlabOauthPage() {
  const redirectUri = `${window.location.origin}/auth/callback`

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4">
      <div>
        <h1 className="text-lg font-semibold">Connecting a GitLab instance</h1>
        <p className="text-sm text-muted-foreground">
          Foundry signs users in through GitLab (OAuth) and inherits their GitLab permissions.
          Connecting an instance takes one OAuth application and the five read-only scopes below.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">1 · Create the OAuth application</CardTitle>
          <CardDescription>
            GitLab accepts OAuth applications in three places — all work the same for Foundry.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2 text-sm">
          <ol className="list-decimal space-y-1.5 pl-5">
            <li>
              Open one of:
              <ul className="mt-1 list-disc space-y-0.5 pl-5 text-muted-foreground">
                <li>
                  <span className="font-medium text-foreground">Admin Area → Applications</span>{" "}
                  (instance-wide; admin only — can be marked “Trusted” to skip the per-user consent
                  screen)
                </li>
                <li>
                  <span className="font-medium text-foreground">
                    Group → Settings → Applications
                  </span>{" "}
                  (group-owned)
                </li>
                <li>
                  <span className="font-medium text-foreground">Profile → Applications</span>{" "}
                  (user-owned; any user can create one)
                </li>
              </ul>
            </li>
            <li>
              Set <span className="font-medium">Redirect URI</span> to{" "}
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">{redirectUri}</code>
            </li>
            <li>
              Check <span className="font-medium">Confidential</span>.
            </li>
            <li>
              Select <span className="font-medium">exactly</span> these scopes:{" "}
              {REQUIRED_SCOPES.map((s) => (
                <Badge key={s.scope} variant="secondary" className="mr-1 font-mono text-[11px]">
                  {s.scope}
                </Badge>
              ))}
            </li>
            <li>
              Save, then copy the <span className="font-medium">Application ID</span> and{" "}
              <span className="font-medium">Secret</span> into{" "}
              <span className="font-medium">Settings → GitLab Instances</span> here.
            </li>
          </ol>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <CheckIcon className="size-4 text-slot-free" aria-hidden />
            Required scopes — and why
          </CardTitle>
          <CardDescription>
            All five are read-only. Foundry requests nothing else. Note: read_api and read_registry
            overlap only for browsing — read_api covers the registry REST API, while read_registry
            authorizes the registry service itself (actual image pulls). Both are required.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-36">Scope</TableHead>
                <TableHead>Why Foundry needs it</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {REQUIRED_SCOPES.map((s) => (
                <TableRow key={s.scope}>
                  <TableCell className="align-top font-mono text-xs">{s.scope}</TableCell>
                  <TableCell className="text-sm">{s.purpose}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <XIcon className="size-4 text-slot-failed" aria-hidden />
            Everything else: leave unchecked
          </CardTitle>
          <CardDescription>
            Least privilege — Foundry never writes to GitLab, never reads source code, and never
            pushes images.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-64">Scope(s)</TableHead>
                <TableHead>Why it is not needed</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {NOT_NEEDED.map((s) => (
                <TableRow key={s.scope}>
                  <TableCell className="align-top font-mono text-xs">{s.scope}</TableCell>
                  <TableCell className="text-sm">{s.reason}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}
