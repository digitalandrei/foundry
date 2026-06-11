import { LogOutIcon, ShieldCheckIcon } from "lucide-react"

import { useLogout } from "@/hooks/use-auth"
import type { MeResponse } from "@/lib/types"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

export function UserMenu({ me }: { me: MeResponse }) {
  const logout = useLogout()
  const initials = me.display_name
    .split(/\s+/)
    .map((p) => p[0])
    .slice(0, 2)
    .join("")
    .toUpperCase()

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" className="gap-2 px-2">
          <Avatar className="size-6">
            {me.avatar_url ? <AvatarImage src={me.avatar_url} alt="" /> : null}
            <AvatarFallback className="text-[10px]">{initials}</AvatarFallback>
          </Avatar>
          <span className="hidden text-sm sm:inline">{me.display_name}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel className="font-normal">
          <p className="text-sm font-medium">{me.display_name}</p>
          {me.email ? <p className="text-xs text-muted-foreground">{me.email}</p> : null}
          {me.is_admin ? (
            <p className="mt-1 flex items-center gap-1 text-xs text-muted-foreground">
              <ShieldCheckIcon className="size-3" aria-hidden /> Administrator
            </p>
          ) : null}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => logout.mutate()} disabled={logout.isPending}>
          <LogOutIcon className="size-4" aria-hidden /> Log out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
