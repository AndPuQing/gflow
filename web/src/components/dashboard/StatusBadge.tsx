import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

export const stateTone: Record<string, string> = {
  Queued: "bg-sky-100 text-sky-800 ring-sky-200 dark:bg-sky-950 dark:text-sky-200",
  Running:
    "bg-emerald-100 text-emerald-800 ring-emerald-200 dark:bg-emerald-950 dark:text-emerald-200",
  Finished:
    "bg-zinc-100 text-zinc-800 ring-zinc-200 dark:bg-zinc-900 dark:text-zinc-200",
  Failed: "bg-rose-100 text-rose-800 ring-rose-200 dark:bg-rose-950 dark:text-rose-200",
  Cancelled:
    "bg-amber-100 text-amber-800 ring-amber-200 dark:bg-amber-950 dark:text-amber-200",
  Timeout:
    "bg-orange-100 text-orange-800 ring-orange-200 dark:bg-orange-950 dark:text-orange-200",
  Available:
    "bg-emerald-100 text-emerald-800 ring-emerald-200 dark:bg-emerald-950 dark:text-emerald-200",
  Busy: "bg-rose-100 text-rose-800 ring-rose-200 dark:bg-rose-950 dark:text-rose-200",
}

export function StatusBadge({ value }: { value: string }) {
  return (
    <Badge className={cn("ring-1", stateTone[value] ?? "bg-muted text-foreground")}>
      {value}
    </Badge>
  )
}
