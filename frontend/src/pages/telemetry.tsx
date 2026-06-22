import { Link } from "@tanstack/react-router"

import { ServerTelemetry } from "@/components/server-telemetry"
import { useServers } from "@/hooks/use-servers"
import { SERVER_STATUS_META } from "@/lib/states"
import { cn } from "@/lib/utils"
import { Skeleton } from "@/components/ui/skeleton"

/** Fleet-wide telemetry (operator requirement): every server's host + GPU
 * graphs, plus per-MIG-slice memory, on one scrollable page. The dashboard
 * keeps the at-a-glance summary; this is the deep-dive. Series are capped
 * at 24h server-side (server_metrics retention). */
export function TelemetryPage() {
  const servers = useServers()

  if (servers.isPending) {
    return (
      <div className="mx-auto flex max-w-6xl flex-col gap-4">
        <Skeleton className="h-7 w-40" />
        <Skeleton className="h-48 w-full" />
      </div>
    )
  }

  const list = (servers.data ?? []).filter((s) => s.enrolled)

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6">
      <h1 className="text-lg font-semibold">Telemetry</h1>
      {list.length === 0 ? (
        <p className="text-sm text-muted-foreground">No enrolled servers yet.</p>
      ) : (
        list.map((server) => (
          <section key={server.id} className="flex flex-col gap-3">
            <div className="flex items-center gap-2 border-b pb-2">
              <span
                className={cn(
                  "size-2.5 rounded-full",
                  SERVER_STATUS_META[server.status].dotClass,
                )}
                aria-hidden
              />
              <Link
                to="/servers/$serverId"
                params={{ serverId: server.id }}
                className="font-medium hover:underline"
              >
                {server.name}
              </Link>
              {server.hostname ? (
                <span className="text-sm text-muted-foreground">{server.hostname}</span>
              ) : null}
              <span className="ml-auto text-xs text-muted-foreground">
                {server.gpus.length} GPU{server.gpus.length === 1 ? "" : "s"}
              </span>
            </div>
            <ServerTelemetry server={server} />
          </section>
        ))
      )}
    </div>
  )
}
