import { useEffect, useLayoutEffect, useRef, useState } from "react"
import { CopyIcon } from "lucide-react"
import { toast } from "sonner"

import { DetailPanel } from "@/components/detail-panel"
import { useDeploymentLogs } from "@/hooks/use-deployments"
import { formatRelative } from "@/lib/format"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Skeleton } from "@/components/ui/skeleton"

/** Console box: the captured container logs (merged stdout+stderr). The
 * Follow toggle and Copy live on the title line (next to Expand); the box
 * body is the scrollable log view. Follow polls every 3s and sticks to
 * the bottom. */
export function ConsolePanel({
  deploymentId,
  expanded,
  onToggle,
  className,
}: {
  deploymentId: string
  expanded: boolean
  onToggle: () => void
  className?: string
}) {
  const [follow, setFollow] = useState(true)
  const logs = useDeploymentLogs(deploymentId, follow)
  const boxRef = useRef<HTMLPreElement>(null)
  const content = logs.data?.content ?? ""

  useLayoutEffect(() => {
    if (follow && boxRef.current) boxRef.current.scrollTop = boxRef.current.scrollHeight
  }, [content, follow])

  useEffect(() => {
    if (boxRef.current) boxRef.current.scrollTop = boxRef.current.scrollHeight
  }, [logs.isSuccess])

  const copy = async () => {
    await navigator.clipboard.writeText(content)
    toast.success("Logs copied")
  }

  const actions = (
    <>
      <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Checkbox checked={follow} onCheckedChange={(v) => setFollow(v === true)} />
        Follow
      </label>
      <Button
        variant="ghost"
        size="icon"
        className="size-7"
        onClick={copy}
        disabled={!content}
        aria-label="Copy logs"
        title="Copy all"
      >
        <CopyIcon className="size-3.5" />
      </Button>
    </>
  )

  return (
    <DetailPanel
      title="Console"
      actions={actions}
      expanded={expanded}
      onToggle={onToggle}
      className={className}
    >
      <div className="min-h-0 flex-1">
        {logs.isPending ? (
          <Skeleton className="h-full min-h-48 w-full" />
        ) : logs.isError ? (
          <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
            Could not load logs.
          </div>
        ) : !logs.data?.available ? (
          <div className="flex h-full items-center justify-center rounded-md border bg-muted/40 p-3 text-center text-xs text-muted-foreground">
            No logs captured yet — they appear once the container produces output and the
            agent uploads them (within ~10s).
          </div>
        ) : (
          <pre
            ref={boxRef}
            className="h-full overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-[11px] leading-relaxed whitespace-pre"
          >
            {content}
          </pre>
        )}
      </div>
      {logs.data?.collected_at ? (
        <p className="shrink-0 pt-1.5 text-right text-[10px] text-muted-foreground">
          last line {formatRelative(logs.data.collected_at)}
        </p>
      ) : null}
    </DetailPanel>
  )
}
