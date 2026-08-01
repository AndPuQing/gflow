import type { Job } from "@/api"
import { formatAssignedGpuIds, formatGpuRequest } from "@/lib/format"
import { cn } from "@/lib/utils"

export function GpuPill({ job }: { job: Job }) {
  const requested = job.gpus ?? 0
  const assignedIds = Array.isArray(job.gpu_ids) ? job.gpu_ids : []
  const assigned = assignedIds.length > 0
  const requestedGpu = requested > 0

  if (!requestedGpu) {
    return (
      <span className="inline-flex h-7 items-center rounded-full border bg-muted px-2.5 font-mono text-xs text-muted-foreground">
        No GPU
      </span>
    )
  }

  const pendingCount = Math.max(requested - assignedIds.length, assigned ? 0 : requested)
  const segments = [
    ...assignedIds.map((id) => ({ key: `gpu-${id}`, label: String(id), state: "assigned" })),
    ...Array.from({ length: pendingCount }, (_, index) => ({
      key: `pending-${index}`,
      label: "…",
      state: "pending",
    })),
  ]

  return (
    <span
      className="inline-flex max-w-[220px] items-center gap-1.5 rounded-full border bg-background p-0.5 align-middle font-mono text-xs shadow-sm"
      title={formatAssignedGpuIds(job)}
      aria-label={`${formatGpuRequest(requested)} ${formatAssignedGpuIds(job)}`}
    >
      <span className="px-1.5 text-muted-foreground">{requested}</span>
      <span className="flex min-w-0 overflow-hidden rounded-full ring-1 ring-border">
        {segments.map((segment) => (
          <span
            key={segment.key}
            className={cn(
              "grid h-5 min-w-6 place-items-center border-r px-1.5 text-[10px] leading-none last:border-r-0",
              segment.state === "assigned"
                ? "border-emerald-200 bg-emerald-100 text-emerald-900 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-100"
                : "border-amber-200 bg-amber-100 text-amber-900 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-100",
            )}
          >
            {segment.label}
          </span>
        ))}
      </span>
    </span>
  )
}
