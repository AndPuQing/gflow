import { cn } from "@/lib/utils"

export const summaryTone = {
  emerald:
    "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200",
  sky: "border-sky-200 bg-sky-50 text-sky-800 dark:border-sky-900 dark:bg-sky-950/40 dark:text-sky-200",
  amber:
    "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200",
  rose: "border-rose-200 bg-rose-50 text-rose-800 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-200",
  zinc: "border-zinc-200 bg-zinc-50 text-zinc-800 dark:border-zinc-800 dark:bg-zinc-900/60 dark:text-zinc-200",
} as const

export type SummaryTone = keyof typeof summaryTone

export function SummaryPill({
  label,
  value,
  tone,
}: {
  label: string
  value: string | number
  tone: SummaryTone
}) {
  return (
    <span
      className={cn(
        "inline-flex h-7 items-center gap-2 rounded-full border px-2.5 text-xs font-medium",
        summaryTone[tone],
      )}
    >
      <span className="text-current/70">{label}</span>
      <span className="font-mono text-sm leading-none">{value}</span>
    </span>
  )
}
