import { useAppTraffic } from "@/hooks/use-deployments"
import { formatFileSize } from "@/lib/volume-files"
import { formatRelative } from "@/lib/format"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function AppTrafficPanel({ deploymentId }: { deploymentId: string }) {
  const { logs, metrics } = useAppTraffic(deploymentId)
  const m = metrics.data
  return (
    <Card>
      <CardHeader><CardTitle className="text-base">Application traffic · last 24 hours</CardTitle></CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-2 text-sm md:grid-cols-5">
          <Metric label="Requests" value={m?.requests.toLocaleString() ?? "—"} />
          <Metric label="5xx" value={m?.errors.toLocaleString() ?? "—"} />
          <Metric label="Average" value={m ? `${m.average_request_time_ms} ms` : "—"} />
          <Metric label="p95" value={m ? `${m.p95_request_time_ms} ms` : "—"} />
          <Metric label="Sent" value={m ? formatFileSize(m.response_bytes) : "—"} />
        </div>
        <div className="max-h-72 overflow-auto rounded-md border">
          <Table>
            <TableHeader><TableRow><TableHead>When</TableHead><TableHead>Request</TableHead><TableHead>Status</TableHead><TableHead>Time</TableHead><TableHead>Bytes</TableHead></TableRow></TableHeader>
            <TableBody>
              {(logs.data ?? []).map((record, index) => (
                <TableRow key={`${record.occurred_at}-${record.request_id ?? index}`}>
                  <TableCell className="whitespace-nowrap text-xs text-muted-foreground">{formatRelative(record.occurred_at)}</TableCell>
                  <TableCell className="max-w-xl truncate font-mono text-xs" title={`${record.method} ${record.path}`}>{record.method} {record.path}</TableCell>
                  <TableCell className={record.status >= 500 ? "text-slot-failed" : record.status >= 400 ? "text-slot-reserved" : "text-slot-free"}>{record.status}</TableCell>
                  <TableCell className="font-mono text-xs">{record.request_time_ms} ms</TableCell>
                  <TableCell className="font-mono text-xs">{formatFileSize(record.response_bytes)}</TableCell>
                </TableRow>
              ))}
              {logs.data?.length === 0 ? <TableRow><TableCell colSpan={5} className="py-8 text-center text-muted-foreground">No proxied requests captured yet.</TableCell></TableRow> : null}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  )
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-md border p-2"><p className="text-xs text-muted-foreground">{label}</p><p className="font-mono font-medium">{value}</p></div>
}
