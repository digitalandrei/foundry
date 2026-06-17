import { KeyRoundIcon, Trash2Icon } from "lucide-react"

import { useConfirm } from "@/components/confirm-context"
import { FleetKeyDialog } from "@/components/fleet-key-dialog"
import { useDeleteFleetToken, useFleetTokens } from "@/hooks/use-servers"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { EmptyState } from "@/components/empty-state"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"

/** Fleet enrollment keys — reusable, expiring keys that auto-enroll any
 * number of agents (each unique by hostname). Many may coexist; each is
 * deletable at any time, even before it expires. Admin-only. */
export function FleetKeysSection() {
  const tokens = useFleetTokens(true)
  const del = useDeleteFleetToken()
  const confirm = useConfirm()

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="text-base">Fleet enrollment keys</CardTitle>
        <FleetKeyDialog />
      </CardHeader>
      <CardContent>
        {tokens.isPending ? (
          <Skeleton className="h-16 w-full" />
        ) : tokens.isError ? (
          <EmptyState icon={KeyRoundIcon} title="Could not load fleet keys" />
        ) : tokens.data.length === 0 ? (
          <EmptyState
            icon={KeyRoundIcon}
            title="No fleet keys"
            description="Mint a key to let agents auto-enroll under their own hostname."
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>Created by</TableHead>
                <TableHead>Expires</TableHead>
                <TableHead>Uses</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tokens.data.map((t) => (
                <TableRow key={t.id}>
                  <TableCell className={t.expired ? "text-muted-foreground" : "text-slot-free"}>
                    {t.expired ? "Expired" : "Active"}
                  </TableCell>
                  <TableCell>{t.created_by_name}</TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(t.expires_at).toLocaleDateString()}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {t.uses}
                    {t.max_uses === null ? " / ∞" : ` / ${t.max_uses}`}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      variant="outline"
                      size="icon"
                      className="size-8 text-slot-failed"
                      disabled={del.isPending}
                      onClick={async () => {
                        if (
                          await confirm({
                            title: "Delete this fleet key?",
                            description:
                              "Agents can no longer enroll with it. Hosts already enrolled stay enrolled.",
                            destructive: true,
                          })
                        )
                          del.mutate(t.id)
                      }}
                      aria-label="Delete fleet key"
                      title="Delete (revoke) this key"
                    >
                      <Trash2Icon className="size-3.5" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}
