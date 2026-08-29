import type { Job } from "@/api"
import { formatAssignedGpuIds, formatGpuRequest } from "@/lib/format"

/**
 * Compact GPU cell for the jobs table: a small "GPU" label followed by one
 * chip per assigned GPU index (emerald) and a single dashed chip summarising
 * how many are still pending. Chips have a fixed height and centered text so
 * they align cleanly with the surrounding row text.
 */
export function GpuPill({ job }: { job: Job }) {
  const requested = job.gpus ?? 0
  const assignedIds = Array.isArray(job.gpu_ids) ? job.gpu_ids : []
  const pending = Math.max(requested - assignedIds.length, 0)

  if (!requested) {
    return <span className="text-xs text-muted-foreground/60">—</span>
  }

  return (
    <span
      className="inline-flex items-center gap-1"
      title={formatAssignedGpuIds(job)}
      aria-label={`${formatGpuRequest(requested)} ${formatAssignedGpuIds(job)}`}
    >
      <span className="font-mono text-[11px] tracking-wide text-muted-foreground">
        {requested}×
      </span>
      {assignedIds.map((id) => (
        <span
          key={`gpu-${id}`}
          className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-md border border-emerald-200 bg-emerald-100 px-1 font-mono text-[11px] font-medium leading-none text-emerald-900 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-100"
        >
          {id}
        </span>
      ))}
      {pending > 0 ? (
        <span
          className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-md border border-dashed border-amber-300 px-1 font-mono text-[11px] leading-none text-amber-700 dark:border-amber-800 dark:text-amber-300"
          title={`${pending} GPU(s) still pending`}
        >
          +{pending}
        </span>
      ) : null}
    </span>
  )
}