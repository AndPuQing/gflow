import type { Job } from "@/api"
import { formatElapsed } from "@/lib/format"

/**
 * Compact progress cell for the jobs table.
 *
 * Only jobs that publish progress (see `gjob progress`) have anything to show,
 * so jobs without it keep a muted dash rather than an empty cell that reads as
 * a rendering bug. A stale document is marked explicitly: reporting an old
 * measurement as if it were current is worse than showing nothing.
 */
export function ProgressCell({ job }: { job: Job }) {
  const progress = job.progress
  if (!progress) {
    return <span className="text-xs text-muted-foreground/60">—</span>
  }

  const percent = typeof progress.percent === "number" ? progress.percent : null
  const total = typeof progress.total === "number" ? progress.total : null
  const eta = typeof progress.eta_secs === "number" ? progress.eta_secs : null
  const title = [
    `${progress.value}${total != null ? `/${total}` : ""} units`,
    progress.rate_per_sec != null
      ? `${progress.rate_per_sec.toFixed(2)} units/s`
      : null,
    eta != null ? `ETA ${eta === 0 ? "done" : formatElapsed(eta)}` : null,
    progress.message,
    progress.stale ? `stale (last update ${formatElapsed(progress.idle_secs)} ago)` : null,
  ]
    .filter(Boolean)
    .join(" · ")

  return (
    <span className="flex min-w-[110px] flex-col gap-1" title={title}>
      <span className="flex items-baseline justify-between gap-2">
        <span className="truncate font-mono text-xs">
          {progress.value}
          {total != null ? `/${total}` : ""}
        </span>
        {percent != null ? (
          <span className="font-mono text-[11px] text-muted-foreground">
            {formatPercent(percent)}
          </span>
        ) : null}
      </span>
      {percent != null ? (
        <span className="h-1 w-full overflow-hidden rounded-full bg-muted">
          <span
            className={
              progress.stale
                ? "block h-full rounded-full bg-amber-400 dark:bg-amber-600"
                : "block h-full rounded-full bg-emerald-500"
            }
            style={{ width: `${percent}%` }}
          />
        </span>
      ) : null}
      {progress.stale ? (
        <span className="text-[10px] leading-none text-amber-700 dark:text-amber-400">
          stale
        </span>
      ) : null}
    </span>
  )
}

/** Whole percentages drop the decimal; fractional ones keep one digit. */
function formatPercent(percent: number) {
  const rounded = Math.round(percent)
  return Math.abs(percent - rounded) < 0.05
    ? `${rounded}%`
    : `${percent.toFixed(1)}%`
}
