import type { UsageStats } from "@/api"
import { formatSeconds } from "@/lib/format"
import { cn } from "@/lib/utils"
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

export function StatsView({ stats }: { stats: UsageStats }) {
  const maxCount = Math.max(
    stats.completed_jobs,
    stats.failed_jobs,
    stats.cancelled_jobs,
    stats.timeout_jobs,
    stats.running_jobs,
    stats.queued_jobs,
    1,
  )
  const jobMix = [
    ["Completed", stats.completed_jobs, "bg-emerald-500"],
    ["Failed", stats.failed_jobs, "bg-rose-500"],
    ["Cancelled", stats.cancelled_jobs, "bg-amber-500"],
    ["Timeout", stats.timeout_jobs, "bg-orange-500"],
    ["Running", stats.running_jobs, "bg-sky-500"],
    ["Queued", stats.queued_jobs, "bg-zinc-500"],
  ] as const

  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_380px]">
      <Card className="rounded-lg">
        <CardHeader className="border-b">
          <CardTitle>Job Mix</CardTitle>
          <CardDescription>{stats.total_jobs} jobs included</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {jobMix.map(([label, value, color]) => (
            <div key={label} className="grid grid-cols-[100px_1fr_48px] items-center gap-3">
              <div className="text-sm text-muted-foreground">{label}</div>
              <div className="h-2 overflow-hidden rounded-full bg-muted">
                <div
                  className={cn("h-full rounded-full", color)}
                  style={{ width: `${(Number(value) / maxCount) * 100}%` }}
                />
              </div>
              <div className="text-right font-mono text-xs">{value}</div>
            </div>
          ))}
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader className="border-b">
          <CardTitle>Runtime</CardTitle>
          <CardDescription>Aggregate timing</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <StatLine label="Average wait" value={formatSeconds(stats.avg_wait_secs)} />
          <StatLine
            label="Average runtime"
            value={formatSeconds(stats.avg_runtime_secs)}
          />
          <StatLine label="Average GPUs/job" value={stats.avg_gpus_per_job.toFixed(2)} />
          <StatLine label="Peak GPU request" value={stats.peak_gpu_usage} />
        </CardContent>
      </Card>

      <Card className="rounded-lg lg:col-span-2">
        <CardHeader className="border-b">
          <CardTitle>Top Runtime Jobs</CardTitle>
          <CardDescription>{stats.top_jobs.length} longest completed runs</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Name</TableHead>
                <TableHead>Runtime</TableHead>
                <TableHead>GPUs</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {stats.top_jobs.length ? (
                stats.top_jobs.map((job) => (
                  <TableRow key={job.id}>
                    <TableCell className="font-mono text-xs">{job.id}</TableCell>
                    <TableCell>{job.name ?? "unnamed"}</TableCell>
                    <TableCell>{formatSeconds(job.runtime_secs)}</TableCell>
                    <TableCell>{job.gpus}</TableCell>
                  </TableRow>
                ))
              ) : (
                <EmptyRow columns={4} label="No runtime data" />
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}

function StatLine({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b pb-3 last:border-b-0 last:pb-0">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="font-mono text-sm">{value}</span>
    </div>
  )
}
