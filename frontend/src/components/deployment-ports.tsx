import { ExternalLinkIcon } from "lucide-react"

import type { DeploymentPort } from "@/lib/types"
import { cn } from "@/lib/utils"

/** First real HTTPS application address for compact surfaces such as a
 * dashboard slot. Stops click propagation so opening the app never also
 * opens the deployment detail route. */
export function DeploymentPrimaryUrl({
  ports,
  className,
}: {
  ports: DeploymentPort[]
  className?: string
}) {
  const hostname =
    ports.find((port) => port.primary && port.hostname)?.hostname ??
    ports.find((port) => port.hostname)?.hostname
  if (!hostname) return null
  const url = `https://${hostname}`
  return (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className={cn(
        "inline-flex min-w-0 items-start gap-0.5 text-left font-mono text-[10px] text-primary underline-offset-2 hover:underline",
        className,
      )}
      title={`Open ${url}`}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <span className="break-all">{url}</span>
      <ExternalLinkIcon className="size-3 shrink-0" aria-hidden />
    </a>
  )
}

/** Shared rendering of a deployment's published ports (deployments
 * table + slot detail dialog): HTTP/S ports are clickable app URLs,
 * everything else is `host→container/proto`. One source so the two
 * call sites never drift (docs/FRONTEND_RULES.md § Structure & Reuse). */
export function DeploymentPorts({
  ports,
  className,
}: {
  ports: DeploymentPort[]
  className?: string
}) {
  if (ports.length === 0) {
    return <span className="text-muted-foreground">—</span>
  }
  return (
    <span className={cn("font-mono text-xs", className)}>
      {ports.map((p) =>
        p.hostname ? (
          <a
            key={`${p.container_port}/${p.protocol}`}
            href={`https://${p.hostname}`}
            target="_blank"
            rel="noreferrer"
            className="mr-1.5 inline-flex items-center gap-0.5 text-primary underline-offset-2 hover:underline"
            title={`https://${p.hostname} → :${p.container_port}`}
          >
            https://{p.hostname}
            <ExternalLinkIcon className="size-3" aria-hidden />
          </a>
        ) : (
          <span key={`${p.container_port}/${p.protocol}`} className="mr-1.5">
            {p.host_port}→{p.container_port}/{p.protocol}
          </span>
        ),
      )}
    </span>
  )
}
