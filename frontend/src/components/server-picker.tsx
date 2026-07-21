import { useMemo } from "react"

import { SearchablePicker } from "@/components/searchable-picker"
import type { ServerSummary } from "@/lib/types"

export function ServerPicker({
  value,
  servers,
  onValueChange,
}: {
  value: string
  servers: ServerSummary[]
  onValueChange: (value: string) => void
}) {
  const options = useMemo(
    () =>
      servers.map((server) => ({
        value: server.id,
        label: server.name,
        searchText: `${server.name} ${server.hostname ?? ""} ${server.status} ${server.id}`,
        content: (
          <span className="block min-w-0">
            <span className="block truncate text-sm font-medium">{server.name}</span>
            <span className="block truncate font-mono text-[10px] text-muted-foreground">
              {server.hostname ?? "hostname unavailable"} · {server.status.toLocaleLowerCase()}
            </span>
          </span>
        ),
      })),
    [servers],
  )

  return (
    <SearchablePicker
      value={value}
      options={options}
      onValueChange={onValueChange}
      ariaLabel="Storage node"
      placeholder="Choose a storage node"
      searchPlaceholder="Search nodes by name or hostname…"
      emptyMessage="No storage nodes match this search."
    />
  )
}
