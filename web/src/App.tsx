import { useEffect, useMemo, useState } from "react"
import {
  Activity,
  Cpu,
  Gauge,
  HardDrive,
  ListFilter,
  RefreshCw,
  Server,
  ShieldAlert,
} from "lucide-react"

import {
  type ApiDuration,
  type ApiResult,
  type ApiTime,
  fetchJson,
  type GpuInfo,
  type IgnoredGpuProcess,
  type Job,
  type Reservation,
  type SchedulerInfo,
  type UsageStats,
  unwrapError,
} from "@/api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"

type DashboardData = {
  info: SchedulerInfo
  jobs: Job[]
  stats: UsageStats
  reservations: Reservation[]
  ignoredProcesses: IgnoredGpuProcess[]
}

const stateTone: Record<string, string> = {
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
}

function App() {
  const [result, setResult] = useState<ApiResult<DashboardData>>({
    data: null,
    error: null,
    loading: true,
  })
  const [query, setQuery] = useState("")

  const load = async () => {
    setResult((current) => ({ ...current, loading: true, error: null }))
    try {
      setResult({ data: await fetchDashboard(), error: null, loading: false })
    } catch (error) {
      setResult({ data: null, error: unwrapError(error), loading: false })
    }
  }

  useEffect(() => {
    let cancelled = false

    fetchDashboard()
      .then((data) => {
        if (!cancelled) {
          setResult({ data, error: null, loading: false })
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setResult({ data: null, error: unwrapError(error), loading: false })
        }
      })

    return () => {
      cancelled = true
    }
  }, [])

  const filteredJobs = useMemo(() => {
    const jobs = result.data?.jobs ?? []
    const needle = query.trim().toLowerCase()
    if (!needle) return jobs

    return jobs.filter((job) => {
      return [
        job.id,
        job.state,
        job.command,
        job.script,
        job.run_name,
        job.submitted_by,
        job.project,
        job.run_dir,
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(needle))
    })
  }, [query, result.data?.jobs])

  const gpus = result.data?.info.gpus ?? []

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
        <header className="flex flex-col gap-3 border-b pb-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Server className="size-4" />
              <span>runqd</span>
            </div>
            <h1 className="mt-1 text-2xl font-semibold tracking-normal text-foreground sm:text-3xl">
              Scheduler Console
            </h1>
          </div>
          <Button onClick={() => void load()} disabled={result.loading} size="sm">
            <RefreshCw
              className={cn("size-4", result.loading && "animate-spin")}
            />
            Refresh
          </Button>
        </header>

        {result.error ? <ErrorState message={result.error} /> : null}

        {result.loading && !result.data ? (
          <LoadingState />
        ) : result.data ? (
          <>
            <Overview data={result.data} />
            <Tabs defaultValue="jobs" className="gap-4">
              <TabsList className="grid w-full grid-cols-4 sm:w-fit">
                <TabsTrigger value="jobs">Jobs</TabsTrigger>
                <TabsTrigger value="gpus">GPUs</TabsTrigger>
                <TabsTrigger value="reservations">Reservations</TabsTrigger>
                <TabsTrigger value="stats">Stats</TabsTrigger>
              </TabsList>

              <TabsContent value="jobs">
                <JobsView jobs={filteredJobs} query={query} onQuery={setQuery} />
              </TabsContent>
              <TabsContent value="gpus">
                <GpuView
                  gpus={gpus}
                  allowed={result.data.info.allowed_gpu_indices}
                  strategy={result.data.info.gpu_allocation_strategy}
                  ignoredProcesses={result.data.ignoredProcesses}
                />
              </TabsContent>
              <TabsContent value="reservations">
                <ReservationsView reservations={result.data.reservations} />
              </TabsContent>
              <TabsContent value="stats">
                <StatsView stats={result.data.stats} />
              </TabsContent>
            </Tabs>
          </>
        ) : null}
      </div>
    </main>
  )
}

async function fetchDashboard(): Promise<DashboardData> {
  const [info, jobs, stats, reservations, ignoredProcesses] = await Promise.all([
    fetchJson<SchedulerInfo>("/info"),
    fetchJson<Job[]>("/jobs?limit=100&order=desc"),
    fetchJson<UsageStats>("/stats"),
    fetchJson<Reservation[]>("/reservations"),
    fetchJson<IgnoredGpuProcess[]>("/gpu-processes"),
  ])

  return { info, jobs, stats, reservations, ignoredProcesses }
}

function Overview({ data }: { data: DashboardData }) {
  const available = data.info.gpus?.filter((gpu) => gpu.available).length ?? 0
  const total = data.info.gpus?.length ?? 0

  return (
    <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
      <MetricCard
        icon={Activity}
        label="Running"
        value={data.stats.running_jobs}
        detail={`${data.stats.queued_jobs} queued`}
      />
      <MetricCard
        icon={Gauge}
        label="Success"
        value={`${data.stats.success_rate.toFixed(1)}%`}
        detail={`${data.stats.completed_jobs} completed`}
      />
      <MetricCard
        icon={Cpu}
        label="GPU Slots"
        value={`${available}/${total}`}
        detail={`${data.stats.peak_gpu_usage} peak request`}
      />
      <MetricCard
        icon={HardDrive}
        label="GPU Hours"
        value={data.stats.total_gpu_hours.toFixed(1)}
        detail={`${data.stats.jobs_with_gpus} GPU jobs`}
      />
    </section>
  )
}

function MetricCard({
  icon: Icon,
  label,
  value,
  detail,
}: {
  icon: typeof Activity
  label: string
  value: string | number
  detail: string
}) {
  return (
    <Card className="rounded-lg">
      <CardHeader>
        <CardDescription className="flex items-center gap-2">
          <Icon className="size-4" />
          {label}
        </CardDescription>
        <CardTitle className="text-2xl">{value}</CardTitle>
      </CardHeader>
      <CardContent className="text-sm text-muted-foreground">{detail}</CardContent>
    </Card>
  )
}

function JobsView({
  jobs,
  query,
  onQuery,
}: {
  jobs: Job[]
  query: string
  onQuery: (value: string) => void
}) {
  return (
    <Card className="rounded-lg">
      <CardHeader className="gap-3 sm:grid-cols-[1fr_auto]">
        <div>
          <CardTitle>Jobs</CardTitle>
          <CardDescription>{jobs.length} visible from the latest page</CardDescription>
        </div>
        <CardAction className="relative w-full sm:w-72">
          <ListFilter className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            placeholder="Filter jobs"
            className="pl-8"
          />
        </CardAction>
      </CardHeader>
      <CardContent>
        <div className="overflow-hidden rounded-lg border">
          <ScrollArea className="h-[520px]">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-20">ID</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>User</TableHead>
                  <TableHead>GPUs</TableHead>
                  <TableHead>Submitted</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {jobs.length ? (
                  jobs.map((job) => (
                    <TableRow key={job.id}>
                      <TableCell className="font-mono text-xs">{job.id}</TableCell>
                      <TableCell>
                        <StatusBadge value={job.state} />
                      </TableCell>
                      <TableCell className="max-w-[360px]">
                        <div className="truncate font-medium">
                          {job.run_name ?? job.command ?? job.script ?? "unnamed"}
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                          {job.project ?? job.run_dir ?? ""}
                        </div>
                      </TableCell>
                      <TableCell>{job.submitted_by ?? "unknown"}</TableCell>
                      <TableCell>{formatGpuRequest(job)}</TableCell>
                      <TableCell>{formatTime(job.submitted_at)}</TableCell>
                    </TableRow>
                  ))
                ) : (
                  <EmptyRow columns={6} label="No jobs match the current filter" />
                )}
              </TableBody>
            </Table>
          </ScrollArea>
        </div>
      </CardContent>
    </Card>
  )
}

function GpuView({
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
  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_380px]">
      <Card className="rounded-lg">
        <CardHeader>
          <CardTitle>GPU Slots</CardTitle>
          <CardDescription>
            {strategy ?? "default"} allocation ·{" "}
            {allowed?.length ? `allowed ${allowed.join(", ")}` : "all allowed"}
          </CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {gpus.length ? (
            gpus.map((gpu) => (
              <div key={gpu.uuid} className="rounded-lg border p-3">
                <div className="flex items-center justify-between gap-3">
                  <div className="font-medium">GPU {gpu.index}</div>
                  <StatusBadge value={gpu.available ? "Available" : "Busy"} />
                </div>
                <div className="mt-2 truncate font-mono text-xs text-muted-foreground">
                  {gpu.uuid}
                </div>
                {gpu.reason ? (
                  <div className="mt-2 text-xs text-muted-foreground">{gpu.reason}</div>
                ) : null}
              </div>
            ))
          ) : (
            <div className="rounded-lg border p-4 text-sm text-muted-foreground">
              No GPU slots reported
            </div>
          )}
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader>
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

function ReservationsView({ reservations }: { reservations: Reservation[] }) {
  return (
    <Card className="rounded-lg">
      <CardHeader>
        <CardTitle>Reservations</CardTitle>
        <CardDescription>{reservations.length} reservations</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-hidden rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>User</TableHead>
                <TableHead>GPU Spec</TableHead>
                <TableHead>Start</TableHead>
                <TableHead>Duration</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {reservations.length ? (
                reservations.map((reservation) => (
                  <TableRow key={reservation.id}>
                    <TableCell className="font-mono text-xs">
                      {reservation.id}
                    </TableCell>
                    <TableCell>
                      <StatusBadge value={reservation.status} />
                    </TableCell>
                    <TableCell>{reservation.user}</TableCell>
                    <TableCell>{formatGpuSpec(reservation.gpu_spec)}</TableCell>
                    <TableCell>{formatTime(reservation.start_time)}</TableCell>
                    <TableCell>{formatDuration(reservation.duration)}</TableCell>
                  </TableRow>
                ))
              ) : (
                <EmptyRow columns={6} label="No reservations" />
              )}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  )
}

function StatsView({ stats }: { stats: UsageStats }) {
  const maxCount = Math.max(
    stats.completed_jobs,
    stats.failed_jobs,
    stats.cancelled_jobs,
    stats.timeout_jobs,
    stats.running_jobs,
    stats.queued_jobs,
    1,
  )

  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_380px]">
      <Card className="rounded-lg">
        <CardHeader>
          <CardTitle>Job Mix</CardTitle>
          <CardDescription>{stats.total_jobs} jobs included</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {[
            ["Completed", stats.completed_jobs],
            ["Failed", stats.failed_jobs],
            ["Cancelled", stats.cancelled_jobs],
            ["Timeout", stats.timeout_jobs],
            ["Running", stats.running_jobs],
            ["Queued", stats.queued_jobs],
          ].map(([label, value]) => (
            <div key={label} className="grid grid-cols-[100px_1fr_48px] items-center gap-3">
              <div className="text-sm text-muted-foreground">{label}</div>
              <div className="h-2 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-foreground"
                  style={{ width: `${(Number(value) / maxCount) * 100}%` }}
                />
              </div>
              <div className="text-right font-mono text-xs">{value}</div>
            </div>
          ))}
        </CardContent>
      </Card>

      <Card className="rounded-lg">
        <CardHeader>
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
        <CardHeader>
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

function StatusBadge({ value }: { value: string }) {
  return (
    <Badge className={cn("ring-1", stateTone[value] ?? "bg-muted text-foreground")}>
      {value}
    </Badge>
  )
}

function EmptyRow({ columns, label }: { columns: number; label: string }) {
  return (
    <TableRow>
      <TableCell colSpan={columns} className="h-24 text-center text-muted-foreground">
        {label}
      </TableCell>
    </TableRow>
  )
}

function ErrorState({ message }: { message: string }) {
  return (
    <Card className="rounded-lg border-rose-200 bg-rose-50 text-rose-950 dark:border-rose-950 dark:bg-rose-950/30 dark:text-rose-100">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ShieldAlert className="size-5" />
          API unavailable
        </CardTitle>
        <CardDescription className="text-rose-800 dark:text-rose-200">
          {message}
        </CardDescription>
      </CardHeader>
    </Card>
  )
}

function LoadingState() {
  return (
    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      {Array.from({ length: 8 }).map((_, index) => (
        <Card key={index} className="rounded-lg">
          <CardHeader>
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-8 w-20" />
          </CardHeader>
          <CardContent>
            <Skeleton className="h-4 w-32" />
          </CardContent>
        </Card>
      ))}
    </div>
  )
}

function formatGpuRequest(job: Job) {
  const ids = Array.isArray(job.gpu_ids) ? ` · ${job.gpu_ids.join(",")}` : ""
  return `${job.gpus ?? 0}${ids}`
}

function formatGpuSpec(value: unknown) {
  if (typeof value === "number" || typeof value === "string") return String(value)
  if (!value || typeof value !== "object") return "unknown"

  const record = value as Record<string, unknown>
  if (typeof record.count === "number") return `${record.count} GPUs`
  if (Array.isArray(record.indices)) return `GPU ${record.indices.join(", ")}`
  if (typeof record.Count === "number") return `${record.Count} GPUs`
  if (Array.isArray(record.Indices)) return `GPU ${record.Indices.join(", ")}`

  return JSON.stringify(value)
}

function formatTime(value?: ApiTime | null) {
  const date = toDate(value)
  if (!date) return "not set"
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date)
}

function formatDuration(value?: ApiDuration | null) {
  if (value == null) return "not set"
  const seconds =
    typeof value === "number" ? value : Number(value.secs ?? 0) + Number(value.nanos ?? 0) / 1e9
  return formatSeconds(seconds)
}

function formatSeconds(value?: number | null) {
  if (value == null || Number.isNaN(value)) return "not set"
  if (value < 60) return `${value.toFixed(1)}s`
  if (value < 3600) return `${(value / 60).toFixed(1)}m`
  return `${(value / 3600).toFixed(1)}h`
}

function toDate(value?: ApiTime | null) {
  if (value == null) return null
  if (typeof value === "string") {
    const date = new Date(value)
    return Number.isNaN(date.valueOf()) ? null : date
  }
  if (typeof value === "number") return new Date(value * 1000)

  const seconds = value.secs_since_epoch ?? value.seconds
  if (seconds == null) return null
  const nanos = value.nanos_since_epoch ?? value.nanos ?? 0
  return new Date(seconds * 1000 + Math.floor(nanos / 1e6))
}

export default App
