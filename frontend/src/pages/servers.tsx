import { useState } from "react"
import { KeyRoundIcon, ServerIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { EnrollServerDialog, RegistrationCommand } from "@/components/enroll-server-dialog"
import { useMe } from "@/hooks/use-auth"
import { useRegenerateToken, useServers } from "@/hooks/use-servers"
import { formatRelative } from "@/lib/format"
import { SERVER_STATUS_META } from "@/lib/states"
import type { EnrollmentTokenResponse } from "@/lib/types"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"

export function ServersPage() {
  const servers = useServers()
  const me = useMe()
  const regenerate = useRegenerateToken()
  const [tokenResult, setTokenResult] = useState<EnrollmentTokenResponse | null>(null)
  const isAdmin = me.data?.is_admin ?? false

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="text-base">Servers</CardTitle>
        {isAdmin ? <EnrollServerDialog /> : null}
      </CardHeader>
      <CardContent>
        {servers.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : servers.isError ? (
          <EmptyState icon={ServerIcon} title="Could not load servers" />
        ) : servers.data.length === 0 ? (
          <EmptyState
            icon={ServerIcon}
            title="No servers enrolled"
            description={
              isAdmin
                ? "Add a server to get its one-time registration command."
                : "An administrator can add GPU servers here."
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>Name</TableHead>
                <TableHead>Hostname</TableHead>
                <TableHead>OS</TableHead>
                <TableHead>Agent</TableHead>
                <TableHead>Last heartbeat</TableHead>
                {isAdmin ? <TableHead className="text-right">Actions</TableHead> : null}
              </TableRow>
            </TableHeader>
            <TableBody>
              {servers.data.map((server) => {
                const meta = SERVER_STATUS_META[server.status]
                return (
                  <TableRow key={server.id}>
                    <TableCell>
                      <span className="flex items-center gap-1.5 text-sm">
                        <span className={cn("size-2.5 rounded-full", meta.dotClass)} aria-hidden />
                        {meta.label}
                      </span>
                    </TableCell>
                    <TableCell className="font-medium">{server.name}</TableCell>
                    <TableCell className="text-muted-foreground">
                      {server.hostname ?? "—"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {server.os_version ?? "—"}
                    </TableCell>
                    <TableCell className="font-mono text-xs">
                      {server.agent_version ? `v${server.agent_version}` : "not enrolled"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {server.last_heartbeat_at ? formatRelative(server.last_heartbeat_at) : "never"}
                    </TableCell>
                    {isAdmin ? (
                      <TableCell className="text-right">
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={regenerate.isPending}
                          onClick={() =>
                            regenerate.mutate(server.id, { onSuccess: setTokenResult })
                          }
                        >
                          <KeyRoundIcon className="size-3.5" aria-hidden />
                          New token
                        </Button>
                      </TableCell>
                    ) : null}
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}

        <Dialog open={tokenResult !== null} onOpenChange={(o) => !o && setTokenResult(null)}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Register the agent</DialogTitle>
              <DialogDescription>
                A fresh token — any previous unused token for this server is now revoked.
              </DialogDescription>
            </DialogHeader>
            {tokenResult ? <RegistrationCommand result={tokenResult} /> : null}
          </DialogContent>
        </Dialog>
      </CardContent>
    </Card>
  )
}
