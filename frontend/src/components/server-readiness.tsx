import { ActivityIcon, RefreshCwIcon, WrenchIcon } from "lucide-react"

import { useRunDiagnostics, useUpgradeAgent } from "@/hooks/use-servers"
import { formatRelative } from "@/lib/format"
import { agentSupportsOperationalSafety } from "@/lib/agent-version"
import { formatFileSize } from "@/lib/volume-files"
import type { ServerSummary } from "@/lib/types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

export function ServerReadiness({ server, admin }: { server: ServerSummary; admin: boolean }) {
  const diagnostics = useRunDiagnostics()
  const upgrade = useUpgradeAgent()
  const storageUsed =
    server.storage_total_bytes != null && server.storage_available_bytes != null
      ? server.storage_total_bytes - server.storage_available_bytes
      : null
  const canRemoteUpgrade = agentSupportsOperationalSafety(server.agent_version)
  return (
    <Card>
      <CardHeader className="flex-row items-center gap-3">
        <CardTitle className="flex items-center gap-2 text-base">
          <ActivityIcon className="size-4" /> Host readiness
        </CardTitle>
        <span className="text-xs text-muted-foreground">
          setup r{server.setup_revision ?? "?"} / r{server.required_setup_revision}
          {server.readiness_checked_at ? ` · checked ${formatRelative(server.readiness_checked_at)}` : ""}
        </span>
        {admin ? (
          <div className="ml-auto flex gap-2">
            <Button variant="outline" size="sm" disabled={diagnostics.isPending} onClick={() => diagnostics.mutate(server.id)}>
              <RefreshCwIcon className="size-3.5" /> Run diagnostics
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={upgrade.isPending || !canRemoteUpgrade}
              title={canRemoteUpgrade ? "Download and reinstall the current agent" : "Bootstrap foundry-agent 0.59.0 once with sudo foundry-agent --setup-apps"}
              onClick={() => upgrade.mutate(server.id)}
            >
              <WrenchIcon className="size-3.5" /> Upgrade & repair
            </Button>
          </div>
        ) : null}
      </CardHeader>
      <CardContent className="space-y-3">
        {storageUsed != null && server.storage_total_bytes != null ? (
          <p className="text-xs text-muted-foreground">
            Persistent storage {formatFileSize(storageUsed)} used · {formatFileSize(server.storage_available_bytes ?? 0)} free
          </p>
        ) : null}
        <div className="grid gap-2 md:grid-cols-2">
          {(server.readiness?.checks ?? []).map((check) => (
            <div key={check.code} className="flex items-start gap-2 rounded-md border p-2 text-xs">
              <Badge variant={check.status === "FAILED" ? "destructive" : "secondary"} className="mt-0.5 text-[9px]">
                {check.status}
              </Badge>
              <div>
                <p className="font-mono font-medium">{check.code}</p>
                <p className="text-muted-foreground">{check.detail}</p>
              </div>
            </div>
          ))}
          {!server.readiness ? <p className="text-sm text-muted-foreground">No live readiness report yet.</p> : null}
        </div>
      </CardContent>
    </Card>
  )
}
