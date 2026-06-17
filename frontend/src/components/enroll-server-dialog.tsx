import { useState } from "react"
import { CheckIcon, CopyIcon, PlusIcon } from "lucide-react"

import { useCreateServer } from "@/hooks/use-servers"
import type { EnrollmentTokenResponse } from "@/lib/types"
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

/** A copyable command line with a one-shot copy button. Shared by the
 * server-token and fleet-key flows. */
export function CopyableCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    await navigator.clipboard.writeText(command)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-md bg-muted p-3 pr-10 font-mono text-xs whitespace-pre-wrap break-all">
        {command}
      </pre>
      <Button
        variant="ghost"
        size="icon"
        className="absolute top-1.5 right-1.5 size-7"
        onClick={copy}
        aria-label="Copy command"
      >
        {copied ? <CheckIcon className="size-3.5" /> : <CopyIcon className="size-3.5" />}
      </Button>
    </div>
  )
}

/** Hint line: how to fetch the agent binary onto a host before enrolling. */
function BinaryHint() {
  return (
    <p className="text-xs text-muted-foreground">
      Get the binary first:{" "}
      <code className="font-mono">
        curl -fsSLo /usr/local/bin/foundry-agent {window.location.origin}/downloads/foundry-agent
        &amp;&amp; chmod +x /usr/local/bin/foundry-agent
      </code>
    </p>
  )
}

/** One-time registration command block (also used by the regenerate
 * flow). The token is only ever shown here. */
export function RegistrationCommand({ result }: { result: EnrollmentTokenResponse }) {
  return (
    <div className="space-y-2">
      <p className="text-sm">
        Run this on <span className="font-medium">{result.server.name}</span> (as root). The token
        is single-use and shown only once; it expires{" "}
        {new Date(result.expires_at).toLocaleString()}.
      </p>
      <CopyableCommand command={result.command} />
      <BinaryHint />
    </div>
  )
}

export function EnrollServerDialog() {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState("")
  const [result, setResult] = useState<EnrollmentTokenResponse | null>(null)
  const create = useCreateServer()

  const reset = () => {
    setName("")
    setResult(null)
    create.reset()
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) reset()
      }}
    >
      <DialogTrigger asChild>
        <Button size="sm">
          <PlusIcon className="size-4" aria-hidden /> Add server
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{result ? "Register the agent" : "Add a GPU server"}</DialogTitle>
          <DialogDescription>
            {result
              ? "The server stays Offline until the agent's first heartbeat."
              : "Names the server and generates its one-time enrollment token (GitLab-agent style)."}
          </DialogDescription>
        </DialogHeader>

        {result ? (
          <RegistrationCommand result={result} />
        ) : (
          <form
            id="add-server-form"
            onSubmit={(e) => {
              e.preventDefault()
              if (name.trim()) {
                create.mutate(name.trim(), { onSuccess: setResult })
              }
            }}
          >
            <div className="space-y-2">
              <Label htmlFor="server-name">Server name</Label>
              <Input
                id="server-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="gpu-server-01"
                autoComplete="off"
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
              form="add-server-form"
              disabled={!name.trim() || create.isPending}
            >
              {create.isPending ? "Creating…" : "Create & get token"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
