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

type GpuStatus = "Available" | "Busy" | "Blocked"

const statusTone: Record<GpuStatus, string> = {
  Available:
    "border-emerald-200 bg-emerald-50/80 dark:border-emerald-900 dark:bg-emerald-950/25",
  Busy: "border-rose-200 bg-rose-50/80 dark:border-rose-900 dark:bg-rose-950/25",
  Blocked: "border-dashed bg-muted/40 opacity-60 grayscale dark:bg-muted/20",
}

const statusDot: Record<GpuStatus, string> = {
  Available: "bg-emerald-500",
  Busy: "bg-rose-500",
  Blocked: "bg-zinc-400 dark:bg-zinc-500",
}

/** Raw daemon reasons can be terse; surface the human-relevant parts. */
function formatGpuReason(reason: string | null | undefined): string | null {
  if (!reason) return null
  const manualIgnore = reason.match(/^manual_ignore\(gpu=\d+,pid=([\d,]+)\)$/)
  if (manualIgnore) return `Manually ignored (pids ${manualIgnore[1]})`
  return reason
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
        <CardContent>
          {gpus.length ? (
            <GpuCardGrid gpus={gpus} allowed={allowed} />
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

function GpuCardGrid({
  gpus,
  allowed,
}: {
  gpus: GpuInfo[]
  allowed?: number[] | null
}) {
  const orderedGpus = [...gpus].sort((left, right) => left.index - right.index)
  const allowedSet = allowed?.length ? new Set(allowed) : null

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      {orderedGpus.map((gpu) => (
        <GpuCard
          key={gpu.uuid}
          gpu={gpu}
          blocked={allowedSet ? !allowedSet.has(gpu.index) : false}
        />
      ))}
    </div>
  )
}

function GpuCard({ gpu, blocked }: { gpu: GpuInfo; blocked: boolean }) {
  const status: GpuStatus = blocked ? "Blocked" : gpu.available ? "Available" : "Busy"
  const reason = formatGpuReason(gpu.reason)
  const detail =
    reason ?? (status === "Available" ? "Idle" : status === "Busy" ? "In use" : "Outside allowed set")

  return (
    <div className={cn("rounded-lg border p-3", statusTone[status])}>
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-2 font-mono text-sm font-semibold">
          <span className={cn("size-2 shrink-0 rounded-full", statusDot[status])} />
          GPU {gpu.index}
        </span>
        <StatusBadge value={status} />
      </div>
      <div className="mt-2 line-clamp-2 min-h-8 text-xs text-muted-foreground">
        {detail}
      </div>
      <div
        className="mt-1 truncate font-mono text-[11px] text-muted-foreground/70"
        title={gpu.uuid}
      >
        {gpu.uuid}
      </div>
    </div>
  )
}