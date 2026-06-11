import { useServerDetail } from "@/hooks/use-servers"
import { formatRelative } from "@/lib/format"
import { Badge } from "@/components/ui/badge"
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

/** Server detail: runtime versions + the docker-ps snapshot. Unmanaged
 * containers are visible but read-only by design (foundry.managed
 * labels — docs/ARCHITECTURE.md § Container Labels). */
export function ServerDetailDialog({
  serverId,
  onClose,
}: {
  serverId: string | null
  onClose: () => void
}) {
  const detail = useServerDetail(serverId)

  return (
    <Dialog open={serverId !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[80svh] overflow-y-auto sm:max-w-2xl">
        {detail.isPending || !detail.data ? (
          <>
            <DialogHeader>
              <DialogTitle>Server detail</DialogTitle>
              <DialogDescription>Loading…</DialogDescription>
            </DialogHeader>
            <Skeleton className="h-32 w-full" />
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>{detail.data.server.name}</DialogTitle>
              <DialogDescription className="flex flex-wrap gap-x-4 gap-y-1">
                {detail.data.server.hostname ? <span>{detail.data.server.hostname}</span> : null}
                <span>Docker {detail.data.docker_version ?? "—"}</span>
                <span>NVIDIA driver {detail.data.nvidia_driver_version ?? "—"}</span>
              </DialogDescription>
            </DialogHeader>

            <div>
              <h3 className="mb-1 text-sm font-medium">
                Containers ({detail.data.containers.length})
              </h3>
              {detail.data.containers.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  No containers reported (or no inventory yet — snapshots arrive every minute).
                </p>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Name</TableHead>
                      <TableHead>Image</TableHead>
                      <TableHead>State</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead />
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {detail.data.containers.map((c) => (
                      <TableRow key={c.container_id}>
                        <TableCell className="font-medium">{c.name}</TableCell>
                        <TableCell
                          className="max-w-48 truncate font-mono text-xs text-muted-foreground"
                          title={c.image}
                        >
                          {c.image}
                        </TableCell>
                        <TableCell
                          className={
                            c.state === "running" ? "text-slot-free" : "text-muted-foreground"
                          }
                        >
                          {c.state}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">{c.status}</TableCell>
                        <TableCell>
                          {c.managed ? <Badge variant="secondary">foundry</Badge> : null}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
              {detail.data.containers[0] ? (
                <p className="mt-1 text-right text-xs text-muted-foreground">
                  snapshot {formatRelative(detail.data.containers[0].reported_at)}
                </p>
              ) : null}
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
