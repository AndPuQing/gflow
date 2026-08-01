import type { GpuInfo, IgnoredGpuProcess } from "@/api"
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
import { EmptyRow } from "@/components/dashboard/StatePanels"
import { StatusBadge } from "@/components/dashboard/StatusBadge"
import { SummaryPill } from "@/components/dashboard/SummaryPill"
import { cn } from "@/lib/utils"

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
  const allowedSet = allowed?.length ? new Set(allowed) : null
  const availableCount = gpus.filter((gpu) => gpu.available).length
  const blockedCount = allowedSet
    ? gpus.filter((gpu) => !allowedSet.has(gpu.index)).length
    : 0

  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_380px]">
      <Card className="rounded-lg">
        <CardHeader className="gap-3 border-b">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div>
              <CardTitle>GPU Slots</CardTitle>
              <CardDescription>
                {strategy ?? "default"} allocation ·{" "}
                {allowed?.length ? `allowed ${allowed.join(", ")}` : "all allowed"}
              </CardDescription>
            </div>
            <div className="flex flex-wrap gap-2">
              <SummaryPill label="Available" value={availableCount} tone="emerald" />
              <SummaryPill label="Busy" value={gpus.length - availableCount} tone="rose" />
              {blockedCount > 0 ? (
                <SummaryPill label="Blocked" value={blockedCount} tone="zinc" />
              ) : null}
            </div>
          </div>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {gpus.length ? (
            <GpuSlotCapsule gpus={gpus} allowed={allowed} />
          ) : (
            <div className="rounded-lg border p-4 text-sm text-muted-foreground">
              No GPU slots reported
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader className="border-b">
          <CardTitle>Ignored Processes</CardTitle>
          <CardDescription>{ignoredProcesses.length} configured</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>GPU</TableHead>
                <TableHead>PID</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {ignoredProcesses.length ? (
                ignoredProcesses.map((process) => (
                  <TableRow key={`${process.gpu_index}-${process.pid}`}>
                    <TableCell>{process.gpu_index}</TableCell>
                    <TableCell className="font-mono text-xs">{process.pid}</TableCell>
                  </TableRow>
                ))
              ) : (
                <EmptyRow columns={2} label="No ignored GPU processes" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}

function GpuSlotCapsule({
  gpus,
  allowed,
}: {
  gpus: GpuInfo[]
  allowed?: number[] | null
}) {
  const orderedGpus = [...gpus].sort((left, right) => left.index - right.index)
  const allowedSet = allowed?.length ? new Set(allowed) : null

  return (
    <div className="col-span-full space-y-3">
      <div className="overflow-x-auto pb-1">
        <div
          className="flex min-w-full overflow-hidden rounded-full border bg-border shadow-sm"
          role="list"
          aria-label="GPU slot availability"
        >
          {orderedGpus.map((gpu) => {
            const allowedGpu = allowedSet ? allowedSet.has(gpu.index) : true
            return (
              <div
                key={gpu.uuid}
                role="listitem"
                title={`${gpu.uuid}${gpu.reason ? ` · ${gpu.reason}` : ""}`}
                className={cn(
                  "flex min-h-16 min-w-24 flex-1 flex-col items-center justify-center gap-1 border-r px-3 text-center last:border-r-0",
                  gpu.available
                    ? "border-emerald-200 bg-emerald-100 text-emerald-950 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-100"
                    : "border-rose-200 bg-rose-100 text-rose-950 dark:border-rose-900 dark:bg-rose-950 dark:text-rose-100",
                  !allowedGpu && "opacity-45 grayscale",
                )}
              >
                <span className="font-mono text-sm font-semibold">GPU {gpu.index}</span>
                <span className="text-[11px] uppercase tracking-wide opacity-75">
                  {allowedGpu ? (gpu.available ? "Available" : "Busy") : "Blocked"}
                </span>
              </div>
            )
          })}
        </div>
      </div>
      <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
        {orderedGpus.map((gpu) => (
          <div key={gpu.uuid} className="rounded-lg border px-3 py-2 text-xs">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium">GPU {gpu.index}</span>
              <StatusBadge value={gpu.available ? "Available" : "Busy"} />
            </div>
            <div className="mt-1 truncate font-mono text-muted-foreground">{gpu.uuid}</div>
            {gpu.reason ? (
              <div className="mt-1 text-muted-foreground">{gpu.reason}</div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  )
}
