import { useState } from "react"
import { LayersIcon } from "lucide-react"

import { CopyableCommand } from "@/components/enroll-server-dialog"
import { useCreateFleetToken } from "@/hooks/use-servers"
import type { FleetTokenResponse } from "@/lib/types"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/** Mint a reusable, time-limited fleet enrollment key. Agents launched
 * with `--fleet-token <key>` auto-enroll under their own hostname, so the
 * key is not tied to any one server. Admin-only. */
export function FleetKeyDialog() {
  const [open, setOpen] = useState(false)
  const [ttlHours, setTtlHours] = useState("12")
  const [maxUses, setMaxUses] = useState("")
  const [result, setResult] = useState<FleetTokenResponse | null>(null)
  const create = useCreateFleetToken()

  const reset = () => {
    setTtlHours("12")
    setMaxUses("")
    setResult(null)
    create.reset()
  }

  const ttl = Number(ttlHours)
  const ttlValid = Number.isInteger(ttl) && ttl >= 1 && ttl <= 24 * 30
  const usesValid = maxUses === "" || (Number.isInteger(Number(maxUses)) && Number(maxUses) >= 1)

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) reset()
      }}
    >
      <DialogTrigger asChild>
        <Button size="sm" variant="outline">
          <LayersIcon className="size-4" aria-hidden /> Fleet key
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{result ? "Fleet enrollment key" : "Mint a fleet key"}</DialogTitle>
          <DialogDescription>
            {result
              ? "Shown only once. Hosts that present it auto-enroll under their own hostname."
              : "A reusable, expiring key for auto-enrolling agents. Each host registers itself under its hostname."}
          </DialogDescription>
        </DialogHeader>

        {result ? (
          <div className="space-y-2">
            <p className="text-sm">
              Run on each fleet host (as root). The key expires{" "}
              {new Date(result.expires_at).toLocaleString()}
              {result.max_uses === null
                ? " — unlimited uses until then."
                : ` — up to ${result.max_uses} host${result.max_uses === 1 ? "" : "s"}.`}
            </p>
            <CopyableCommand command={result.command} />
          </div>
        ) : (
          <form
            id="fleet-key-form"
            onSubmit={(e) => {
              e.preventDefault()
              if (ttlValid && usesValid) {
                create.mutate(
                  { ttl_hours: ttl, max_uses: maxUses === "" ? null : Number(maxUses) },
                  { onSuccess: setResult },
                )
              }
            }}
            className="space-y-4"
          >
            <div className="space-y-2">
              <Label htmlFor="fleet-ttl">Expires in (hours)</Label>
              <Input
                id="fleet-ttl"
                type="number"
                min={1}
                max={720}
                value={ttlHours}
                onChange={(e) => setTtlHours(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="fleet-max-uses">Max uses (blank = unlimited)</Label>
              <Input
                id="fleet-max-uses"
                type="number"
                min={1}
                value={maxUses}
                onChange={(e) => setMaxUses(e.target.value)}
                placeholder="unlimited"
              />
            </div>
          </form>
        )}

        <DialogFooter>
          {result ? (
            <Button onClick={() => setOpen(false)}>Done</Button>
          ) : (
            <Button
              type="submit"
              form="fleet-key-form"
              disabled={!ttlValid || !usesValid || create.isPending}
            >
              {create.isPending ? "Minting…" : "Mint key"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
