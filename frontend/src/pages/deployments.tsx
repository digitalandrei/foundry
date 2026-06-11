import { RocketIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

export function DeploymentsPage() {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Deployments</CardTitle>
      </CardHeader>
      <CardContent>
        <EmptyState
          icon={RocketIcon}
          title="No deployments yet"
          description="Drag a container image onto a free GPU slot on the Dashboard to create the first one."
        />
      </CardContent>
    </Card>
  )
}
