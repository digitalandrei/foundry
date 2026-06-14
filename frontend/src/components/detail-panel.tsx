import { Maximize2Icon, Minimize2Icon } from "lucide-react"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"

/** A bottom box on the deployment page (Console / Shell): a header line
 * with the title on the left and the box's actions + an expand-to-full-
 * width toggle on the right (lg only — phones stack both full-width). */
export function DetailPanel({
  title,
  actions,
  expanded,
  onToggle,
  className,
  children,
}: {
  title: string
  actions?: React.ReactNode
  expanded: boolean
  onToggle: () => void
  className?: string
  children: React.ReactNode
}) {
  return (
    <Card className={cn("flex min-h-0 flex-col gap-0 py-0", className)}>
      <div className="flex shrink-0 items-center justify-between gap-2 border-b px-4 py-2.5">
        <p className="text-base font-medium">{title}</p>
        <div className="flex items-center gap-1.5">
          {actions}
          <Button
            variant="ghost"
            size="icon"
            className="hidden size-7 lg:inline-flex"
            onClick={onToggle}
            aria-label={expanded ? `Restore ${title}` : `Expand ${title}`}
            title={expanded ? "Restore split view" : "Expand to full width"}
          >
            {expanded ? (
              <Minimize2Icon className="size-3.5" />
            ) : (
              <Maximize2Icon className="size-3.5" />
            )}
          </Button>
        </div>
      </div>
      <CardContent className="flex min-h-0 flex-1 flex-col p-3">{children}</CardContent>
    </Card>
  )
}
