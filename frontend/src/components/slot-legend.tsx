import { LEGEND_STATES, SLOT_STATE_META } from "@/lib/states"
import { cn } from "@/lib/utils"

/** Slot-state legend from the dashboard mockup header. */
export function SlotLegend() {
  return (
    <div className="flex items-center gap-4">
      {LEGEND_STATES.map((state) => {
        const meta = SLOT_STATE_META[state]
        return (
          <span key={state} className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span className={cn("size-2.5 rounded-[3px]", meta.dotClass)} aria-hidden />
            {meta.label}
          </span>
        )
      })}
    </div>
  )
}
