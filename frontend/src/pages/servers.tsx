import { ServerIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

export function ServersPage() {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Servers</CardTitle>
      </CardHeader>
      <CardContent>
        <EmptyState
          icon={ServerIcon}
          title="No servers enrolled"
          description="GPU servers join the fleet via single-use enrollment tokens; the agent only ever connects outbound."
        />
      </CardContent>
    </Card>
  )
}
