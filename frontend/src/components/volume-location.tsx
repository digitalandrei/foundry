import { HardDriveIcon } from "lucide-react"

import type { ServerVolume } from "@/lib/types"
import { volumeLocation } from "@/lib/volume-locations"
import { cn } from "@/lib/utils"

export function VolumeLocationLabel({
  volume,
  server,
  includeServer = false,
  className,
}: {
  volume: ServerVolume
  server: { name: string; hostname?: string | null }
  includeServer?: boolean
  className?: string
}) {
  const location = volumeLocation(volume, server)
  const text = includeServer
    ? location.breadcrumb
    : `${location.placement} / ${location.project} / ${location.mount}`
  return (
    <span className={cn("flex min-w-0 items-center gap-1.5", className)} title={location.breadcrumb}>
      <HardDriveIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
      <span className="truncate font-mono text-xs">{text}</span>
    </span>
  )
}
