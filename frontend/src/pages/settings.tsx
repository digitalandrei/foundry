import { useTheme } from "next-themes"

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

const THEMES = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "System" },
] as const

export function SettingsPage() {
  // No SSR here, so next-themes has the stored theme from the first
  // render — no mounted-flag dance needed.
  const { theme, setTheme } = useTheme()

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Appearance</CardTitle>
          <CardDescription>
            Dark is the default; both themes are first-class (docs/UI-DESIGN.md).
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="theme-select">Theme</Label>
            <Select value={theme} onValueChange={setTheme}>
              <SelectTrigger id="theme-select" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {THEMES.map((t) => (
                  <SelectItem key={t.value} value={t.value}>
                    {t.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">GitLab Instances</CardTitle>
          <CardDescription>
            Foundry onboards one or more GitLab instances; users log in through them and inherit
            their GitLab permissions. Instance onboarding opens up with the authentication
            milestone.
          </CardDescription>
        </CardHeader>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Server Enrollment</CardTitle>
          <CardDescription>
            GPU servers enroll with single-use tokens generated here once agent enrollment lands.
          </CardDescription>
        </CardHeader>
      </Card>
    </div>
  )
}
