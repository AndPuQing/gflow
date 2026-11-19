import type { GpuInfo, IgnoredGpuProcess } from "@/api"
import { summaryTone, type SummaryTone } from "@/components/dashboard/SummaryPill"
import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { gpuSlotStatus, type GpuSlotStatus } from "@/lib/format"
import { cn } from "@/lib/utils"

const statusStyle: Record<GpuSlotStatus, { dot: string; tone: SummaryTone }> = {
  Available: { dot: "bg-emerald-500", tone: "emerald" },
  Busy: { dot: "bg-amber-500", tone: "amber" },
  Blocked: { dot: "bg-zinc-400 dark:bg-zinc-500", tone: "zinc" },
  Unavailable: { dot: "bg-rose-500", tone: "rose" },
}

function formatGpuReason(reason: string) {
  if (reason === "nvml_query_failed") return "NVML check failed"

  const process = reason.match(/^(unmanaged|manual_ignore)\((?:gpu=\d+,)?pid=(\d+(?:,\d+)*)\)$/)
  if (!process) return reason

  const pids = process[2].split(",")
  const owner = process[1] === "unmanaged" ? "External" : "Ignored"
  if (pids.length === 1) return `${owner} process · PID ${pids[0]}`
  return `${owner} processes · PIDs ${pids.join(", ")}`
}

export function GpuView({
  gpus,
  allowed,
  strategy,
  ignoredProcesses,
}: {
  gpus: GpuInfo[]
  allowed?: number[] | null
  strategy?: string
  ignoredProcesses: IgnoredGpuProcess[]
}) {
  const allowedSet = allowed == null ? null : new Set(allowed)
  const counts: Record<GpuSlotStatus, number> = { Available: 0, Busy: 0, Blocked: 0, Unavailable: 0 }
  const slots = [...gpus]
    .sort((left, right) => left.index - right.index)
    .map((gpu) => {
      const status = gpuSlotStatus(gpu, allowedSet)
      counts[status] += 1
      return { gpu, status }
    })

  let allowedLabel = "All GPUs allowed"
  if (allowed) {
    allowedLabel = allowed.length ? `GPUs ${allowed.join(", ")} allowed` : "No GPUs allowed"
  }

  return (
    <div className="grid gap-4">
      <Card className="min-w-0 gap-0 rounded-lg py-0 shadow-xs">
        <CardHeader className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3 border-b px-4 pt-4 sm:px-5">
          <div className="min-w-0">
            <CardTitle className="font-semibold">
              <h2>GPU Slots</h2>
            </CardTitle>
            <CardDescription className="mt-0.5">
              <span className="capitalize">{strategy ?? "default"}</span> allocation · {allowedLabel}
            </CardDescription>
          </div>
          {slots.length > 0 && (
            <dl className="flex flex-wrap items-center gap-x-4 gap-y-2">
              {(Object.keys(counts) as GpuSlotStatus[]).map((status) => {
                const count = counts[status]
                if (count === 0 && status !== "Available" && status !== "Busy") return null

                return (
                  <div key={status} className="flex items-center gap-1.5">
                    <dt className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <span
                        aria-hidden="true"
                        className={cn("size-1.5 rounded-full", statusStyle[status].dot)}
                      />
                      {status}
                    </dt>
                    <dd className="text-sm font-semibold tabular-nums">{count}</dd>
                  </div>
                )
              })}
            </dl>
          )}
        </CardHeader>
        <CardContent className="bg-muted/30 p-4 sm:p-5">
          {slots.length ? (
            <ul
              className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-3"
              role="list"
            >
              {slots.map(({ gpu, status }) => (
                <GpuCard key={gpu.uuid} gpu={gpu} status={status} />
              ))}
            </ul>
          ) : (
            <p className="rounded-lg border border-dashed bg-card p-4 text-sm text-muted-foreground">
              No GPU slots reported
            </p>
          )}
        </CardContent>
      </Card>

      <Card className="min-w-0 gap-0 rounded-lg py-0 shadow-xs">
        <CardHeader className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 px-4 py-3 sm:px-5">
          <CardTitle className="text-sm font-medium">
            <h2>Ignored GPU processes</h2>
          </CardTitle>
          <CardDescription className="text-xs">
            {ignoredProcesses.length ? `${ignoredProcesses.length} configured` : "None configured"}
          </CardDescription>
        </CardHeader>
        {ignoredProcesses.length > 0 && (
          <CardContent className="border-t px-4 sm:px-5">
            <Table aria-label="Ignored GPU processes" className="table-fixed">
              <TableHeader>
                <TableRow>
                  <TableHead>GPU</TableHead>
                  <TableHead>PID</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {ignoredProcesses.map((process) => (
                  <TableRow key={`${process.gpu_index}-${process.pid}`}>
                    <TableCell>GPU {process.gpu_index}</TableCell>
                    <TableCell className="font-mono text-xs">{process.pid}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        )}
      </Card>
    </div>
  )
}

function GpuCard({ gpu, status }: { gpu: GpuInfo; status: GpuSlotStatus }) {
  const { dot, tone } = statusStyle[status]
  const reason = gpu.reason ? formatGpuReason(gpu.reason) : null
  const shortUuid = gpu.uuid.length > 20 ? `${gpu.uuid.slice(0, 12)}…${gpu.uuid.slice(-4)}` : gpu.uuid

  return (
    <li className="flex min-w-0 flex-col overflow-hidden rounded-lg border bg-card shadow-xs">
      <div className="grid gap-2 px-3.5 py-3">
        <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
          <h3 className="text-base font-semibold tracking-tight tabular-nums">GPU {gpu.index}</h3>
          <Badge variant="outline" className={cn("h-6 gap-1.5", summaryTone[tone])}>
            <span aria-hidden="true" className={cn("size-1.5 rounded-full", dot)} />
            {status}
          </Badge>
        </div>
        {status === "Blocked" && (
          <p className="text-xs/relaxed text-muted-foreground">Outside the GPU allowlist</p>
        )}
        {reason && <p className="text-xs/relaxed wrap-anywhere text-muted-foreground">{reason}</p>}
      </div>
      <details className="group/uuid mt-auto border-t bg-muted/30">
        <summary
          aria-label={`UUID for GPU ${gpu.index}`}
          title={gpu.uuid}
          className="flex min-h-10 cursor-pointer list-none items-center gap-2 px-3.5 text-xs text-muted-foreground hover:bg-muted/60 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden"
        >
          <span className="font-medium">UUID</span>
          <span
            aria-hidden="true"
            className="ml-auto min-w-0 truncate font-mono text-[11px] group-open/uuid:invisible"
          >
            {shortUuid}
          </span>
          <span
            aria-hidden="true"
            className="mb-0.5 size-1.5 shrink-0 rotate-45 border-r border-b border-current group-open/uuid:mt-1 group-open/uuid:-rotate-135"
          />
        </summary>
        <p className="px-3.5 pb-3 font-mono text-xs wrap-anywhere select-all">{gpu.uuid}</p>
      </details>
    </li>
  )
}
