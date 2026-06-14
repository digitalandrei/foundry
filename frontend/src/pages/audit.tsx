import { useState } from "react"
import { ScrollTextIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { useAuditLog } from "@/hooks/use-audit"
import { AUDIT_ACTION_OPTIONS, auditActionMeta } from "@/lib/audit"
import type { AuditLogEntry } from "@/lib/types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"

export function AuditPage() {
  const [action, setAction] = useState<string | null>(null)
  const audit = useAuditLog(action)
  const entries = audit.data?.pages.flatMap((p) => p.entries) ?? []

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <CardTitle className="text-base">Audit Logs</CardTitle>
          <Select
            value={action ?? "all"}
            onValueChange={(v) => setAction(v === "all" ? null : v)}
          >
            <SelectTrigger className="w-[200px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All actions</SelectItem>
              {AUDIT_ACTION_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </CardHeader>
      <CardContent>
        {audit.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        ) : audit.isError ? (
          <EmptyState icon={ScrollTextIcon} title="Could not load audit logs" />
        ) : entries.length === 0 ? (
          <EmptyState
            icon={ScrollTextIcon}
            title="No audit entries"
            description="Every login, enrollment, deployment action, and settings change is recorded here, append-only."
          />
        ) : (
          <div className="space-y-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Time</TableHead>
                  <TableHead>Actor</TableHead>
                  <TableHead>Action</TableHead>
                  <TableHead>Subject</TableHead>
                  <TableHead>Source IP</TableHead>
                  <TableHead>Detail</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {entries.map((e) => (
                  <AuditRow key={e.id} entry={e} />
                ))}
              </TableBody>
            </Table>
            {audit.hasNextPage ? (
              <div className="flex justify-center">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => audit.fetchNextPage()}
                  disabled={audit.isFetchingNextPage}
                >
                  {audit.isFetchingNextPage ? "Loading…" : "Load more"}
                </Button>
              </div>
            ) : null}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function AuditRow({ entry }: { entry: AuditLogEntry }) {
  const meta = auditActionMeta(entry.action)
  const actor =
    entry.actor_name ??
    (entry.actor_type === "USER" ? "Unknown user" : entry.actor_type.toLowerCase())
  const subject =
    entry.subject_type !== null
      ? `${entry.subject_type}${entry.subject_id ? ` · ${entry.subject_id.slice(0, 8)}` : ""}`
      : "—"
  const detail = entry.detail ? JSON.stringify(entry.detail) : null

  return (
    <TableRow>
      <TableCell className="whitespace-nowrap text-muted-foreground">
        {new Date(entry.created_at).toLocaleString()}
      </TableCell>
      <TableCell className="font-medium">{actor}</TableCell>
      <TableCell>
        <Badge variant={meta.variant}>{meta.label}</Badge>
      </TableCell>
      <TableCell className="text-muted-foreground">{subject}</TableCell>
      <TableCell className="text-muted-foreground">{entry.ip_address ?? "—"}</TableCell>
      <TableCell
        className="max-w-[28rem] truncate text-muted-foreground"
        title={detail ?? undefined}
      >
        {detail ? <code className="text-xs">{detail}</code> : "—"}
      </TableCell>
    </TableRow>
  )
}
