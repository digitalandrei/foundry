import { ScrollTextIcon } from "lucide-react"

import { EmptyState } from "@/components/empty-state"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

export function AuditPage() {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Audit Logs</CardTitle>
      </CardHeader>
      <CardContent>
        <EmptyState
          icon={ScrollTextIcon}
          title="No audit entries"
          description="Every login, enrollment, deployment action, and settings change will be recorded here, append-only."
        />
      </CardContent>
    </Card>
  )
}
