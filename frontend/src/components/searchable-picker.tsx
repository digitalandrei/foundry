import { useEffect, useId, useMemo, useRef, useState } from "react"
import { CheckIcon, ChevronsUpDownIcon, SearchIcon } from "lucide-react"
import { Popover as PopoverPrimitive } from "radix-ui"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { searchablePickerMatches } from "@/lib/searchable-picker"
import { cn } from "@/lib/utils"

export type SearchablePickerOption = {
  value: string
  /** Plain text used by the trigger and accessibility tree. */
  label: string
  /** Additional terms such as hostname, placement, project, and mount. */
  searchText?: string
  /** First hierarchy level. Adjacent equal groups share one heading. */
  group?: string
  /** Second hierarchy level. Adjacent equal subgroups share one heading. */
  subgroup?: string
  /** Rich option body; defaults to `label`. */
  content?: React.ReactNode
}

/** Search-first hierarchical picker used for nodes and persistent roots.
 * Focus remains in the search field while arrow keys move the active option,
 * so large fleets remain fast to navigate with either keyboard or mouse. */
export function SearchablePicker({
  value,
  options,
  onValueChange,
  ariaLabel,
  placeholder,
  searchPlaceholder,
  emptyMessage = "No matches.",
  className,
}: {
  value: string
  options: SearchablePickerOption[]
  onValueChange: (value: string) => void
  ariaLabel: string
  placeholder: string
  searchPlaceholder: string
  emptyMessage?: string
  className?: string
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const [active, setActive] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const listboxId = useId()
  const selected = options.find((option) => option.value === value)
  const filtered = useMemo(
    () => options.filter((option) => searchablePickerMatches(option, query)),
    [options, query],
  )

  useEffect(() => {
    if (!open || !filtered[active]) return
    document.getElementById(`${listboxId}-${active}`)?.scrollIntoView?.({ block: "nearest" })
  }, [active, filtered, listboxId, open])

  const changeOpen = (next: boolean) => {
    setOpen(next)
    if (next) {
      setQuery("")
      const selectedIndex = options.findIndex((option) => option.value === value)
      setActive(Math.max(0, selectedIndex))
    }
  }

  const choose = (next: string) => {
    onValueChange(next)
    setOpen(false)
  }

  return (
    <PopoverPrimitive.Root open={open} onOpenChange={changeOpen}>
      <PopoverPrimitive.Trigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-label={ariaLabel}
          aria-expanded={open}
          aria-controls={open ? listboxId : undefined}
          className={cn("w-full justify-between font-normal", className)}
        >
          <span
            className={cn("min-w-0 truncate", !selected && "text-muted-foreground")}
            title={selected?.label}
          >
            {selected?.label ?? placeholder}
          </span>
          <ChevronsUpDownIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        </Button>
      </PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          align="start"
          sideOffset={4}
          className="z-50 w-[var(--radix-popover-trigger-width)] max-w-[calc(100vw-2rem)] min-w-72 overflow-hidden rounded-lg bg-popover text-popover-foreground shadow-md ring-1 ring-foreground/10"
          onOpenAutoFocus={(event) => {
            event.preventDefault()
            inputRef.current?.focus()
          }}
        >
          <span className="sr-only" aria-live="polite">
            {filtered.length} result{filtered.length === 1 ? "" : "s"}
          </span>
          <div className="relative border-b p-2">
            <SearchIcon
              className="pointer-events-none absolute top-1/2 left-4 size-3.5 -translate-y-1/2 text-muted-foreground"
              aria-hidden
            />
            <Input
              ref={inputRef}
              type="search"
              value={query}
              placeholder={searchPlaceholder}
              aria-label={searchPlaceholder}
              aria-controls={listboxId}
              aria-activedescendant={filtered[active] ? `${listboxId}-${active}` : undefined}
              className="h-8 pl-8 text-xs"
              onChange={(event) => {
                setQuery(event.target.value)
                setActive(0)
              }}
              onKeyDown={(event) => {
                if (filtered.length === 0) return
                if (event.key === "ArrowDown") {
                  event.preventDefault()
                  setActive((index) => (index + 1) % filtered.length)
                } else if (event.key === "ArrowUp") {
                  event.preventDefault()
                  setActive((index) => (index - 1 + filtered.length) % filtered.length)
                } else if (event.key === "Home") {
                  event.preventDefault()
                  setActive(0)
                } else if (event.key === "End") {
                  event.preventDefault()
                  setActive(filtered.length - 1)
                } else if (event.key === "Enter") {
                  event.preventDefault()
                  choose(filtered[active]?.value ?? filtered[0].value)
                }
              }}
            />
          </div>
          <div
            id={listboxId}
            role="listbox"
            aria-label={`${ariaLabel} results`}
            className="max-h-80 overflow-y-auto p-1"
          >
            {filtered.length === 0 ? (
              <p className="px-3 py-8 text-center text-xs text-muted-foreground">{emptyMessage}</p>
            ) : (
              filtered.map((option, index) => {
                const previous = filtered[index - 1]
                const startsGroup = option.group && option.group !== previous?.group
                const startsSubgroup =
                  option.subgroup && (startsGroup || option.subgroup !== previous?.subgroup)
                const optionId = `${listboxId}-${index}`
                return (
                  <div key={option.value}>
                    {startsGroup ? (
                      <div
                        role="presentation"
                        className="mt-1 px-2 py-1 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase first:mt-0"
                      >
                        {option.group}
                      </div>
                    ) : null}
                    {startsSubgroup ? (
                      <div role="presentation" className="px-3 py-1 text-xs font-medium">
                        {option.subgroup}
                      </div>
                    ) : null}
                    <button
                      id={optionId}
                      type="button"
                      role="option"
                      aria-selected={option.value === value}
                      className={cn(
                        "flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm outline-none",
                        option.subgroup && "pl-6",
                        index === active && "bg-accent text-accent-foreground",
                      )}
                      onMouseMove={() => setActive(index)}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => choose(option.value)}
                    >
                      <span className="min-w-0 flex-1">
                        {option.content ? (
                          <>
                            <span className="sr-only">{option.label}</span>
                            <span aria-hidden>{option.content}</span>
                          </>
                        ) : option.label}
                      </span>
                      {option.value === value ? (
                        <CheckIcon className="size-4 shrink-0" aria-hidden />
                      ) : null}
                    </button>
                  </div>
                )
              })
            )}
          </div>
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}
