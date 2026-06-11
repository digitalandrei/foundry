import { Navigate, useSearch } from "@tanstack/react-router"
import { ExternalLinkIcon } from "lucide-react"

import { LocalLoginForm } from "@/components/local-login-form"
import { useMe } from "@/hooks/use-auth"
import { useInstances } from "@/hooks/use-instances"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"

/** Instance picker → GitLab OAuth (docs/GITLAB-INTEGRATION.md § OAuth).
 * Auto-redirect is intentionally avoided even with one instance so a
 * failed login can't loop. */
export function LoginPage() {
  const me = useMe()
  const instances = useInstances()
  const search = useSearch({ from: "/login" })

  if (me.data) {
    return <Navigate to="/" />
  }

  return (
    <div className="flex min-h-svh items-center justify-center p-4">
      <Card className="w-full max-w-sm">
        <CardHeader className="text-center">
          <div className="mx-auto mb-2 flex size-10 items-center justify-center rounded-lg bg-primary font-bold text-primary-foreground">
            F
          </div>
          <CardTitle>Foundry</CardTitle>
          <CardDescription>
            GPU orchestration. Sign in with your GitLab account — your GitLab permissions decide
            what you can see and deploy.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          {search.error ? (
            <p className="rounded-md border border-slot-failed/40 bg-slot-failed/10 p-2 text-center text-sm text-slot-failed">
              Login failed — please try again.
            </p>
          ) : null}

          {instances.isPending ? (
            <>
              <Skeleton className="h-9 w-full" />
              <Skeleton className="h-9 w-full" />
            </>
          ) : instances.isError ? (
            <p className="text-center text-sm text-muted-foreground">
              Could not load GitLab instances. Is the controller running?
            </p>
          ) : instances.data.length === 0 ? (
            <p className="text-center text-sm text-muted-foreground">
              No GitLab instances are onboarded yet. An operator can add the first one on the
              controller host:{" "}
              <code className="font-mono text-xs">foundry-controller instance add …</code>
            </p>
          ) : (
            instances.data.map((instance) => (
              <Button
                key={instance.id}
                className="w-full justify-between"
                onClick={() => window.location.assign(`/auth/login/${instance.id}`)}
              >
                Sign in with {instance.name}
                <ExternalLinkIcon className="size-4" aria-hidden />
              </Button>
            ))
          )}

          <div className="my-2 flex items-center gap-3">
            <Separator className="flex-1" />
            <span className="text-xs text-muted-foreground">operator</span>
            <Separator className="flex-1" />
          </div>
          <LocalLoginForm />
        </CardContent>
      </Card>
    </div>
  )
}
