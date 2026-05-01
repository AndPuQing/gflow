import { type ReactNode, useEffect, useMemo, useState } from "react"
import {
  type ColumnDef,
  type ColumnFiltersState,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  type Row,
  type SortingState,
  type Table as TanStackTable,
  useReactTable,
} from "@tanstack/react-table"
import {
  Activity,
  ArrowDownAZ,
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

type JobTableColumnId =
  | "id"
  | "state"
  | "name"
  | "user"
  | "gpu"
  | "submitted"
type GpuFilter = "all" | "requested" | "none" | "assigned" | "pending"
type SortDirection = "asc" | "desc"

function App() {
  const [result, setResult] = useState<ApiResult<DashboardData>>({
    data: null,
    error: null,
    loading: true,
  })

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
                <JobsView jobs={result.data.jobs} />
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

function JobsView({ jobs }: { jobs: Job[] }) {
  const [query, setQuery] = useState("")
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([])
  const [sorting, setSorting] = useState<SortingState>([{ id: "id", desc: true }])

  const states = useMemo(() => uniqueSorted(jobs.map((job) => job.state)), [jobs])
  const users = useMemo(
    () => uniqueSorted(jobs.map((job) => job.submitted_by ?? "unknown")),
    [jobs],
  )

  const columns = useMemo<ColumnDef<Job>[]>(
    () => [
      {
        accessorKey: "id",
        header: "ID",
        cell: ({ row }) => (
          <span className="font-mono text-xs">{row.original.id}</span>
        ),
        sortingFn: "basic",
      },
      {
        accessorKey: "state",
        header: "Status",
        cell: ({ row }) => <StatusBadge value={row.original.state} />,
        filterFn: exactFilter,
      },
      {
        id: "name",
        accessorFn: jobName,
        header: "Name",
        cell: ({ row }) => (
          <div className="max-w-[360px]">
            <div className="truncate font-medium">{jobName(row.original)}</div>
            <div className="truncate text-xs text-muted-foreground">
              {jobContext(row.original)}
            </div>
          </div>
        ),
      },
      {
        id: "user",
        accessorFn: (job) => job.submitted_by ?? "unknown",
        header: "User",
        filterFn: exactFilter,
      },
      {
        id: "gpu",
        accessorFn: (job) => gpuSortValue(job),
        header: "GPU",
        cell: ({ row }) => <GpuPill job={row.original} />,
        filterFn: gpuStateFilter,
        sortingFn: "basic",
      },
      {
        id: "submitted",
        accessorFn: (job) => toDate(job.submitted_at)?.valueOf() ?? 0,
        header: "Submitted",
        cell: ({ row }) => formatTime(row.original.submitted_at),
        sortingFn: "basic",
      },
    ],
    [],
  )

  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data: jobs,
    columns,
    state: {
      columnFilters,
      globalFilter: query,
      sorting,
    },
    onColumnFiltersChange: setColumnFilters,
    onGlobalFilterChange: setQuery,
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getSortedRowModel: getSortedRowModel(),
    globalFilterFn: jobGlobalFilter,
  })

  const visibleRows = table.getRowModel().rows
  const stateFilter = stringColumnFilter(table, "state")
  const userFilter = stringColumnFilter(table, "user")
  const gpuFilter = stringColumnFilter(table, "gpu") as GpuFilter
  const activeSort = sorting[0]
  const sortField = (activeSort?.id ?? "id") as JobTableColumnId
  const sortDirection: SortDirection = activeSort?.desc === false ? "asc" : "desc"

  const setColumnFilter = (columnId: JobTableColumnId, value: string) => {
    table.getColumn(columnId)?.setFilterValue(value === "all" ? undefined : value)
  }

  const setSortField = (columnId: JobTableColumnId) => {
    setSorting([{ id: columnId, desc: sortDirection === "desc" }])
  }

  const setSortDirection = (direction: SortDirection) => {
    setSorting([{ id: sortField, desc: direction === "desc" }])
  }

  return (
    <Card className="rounded-lg">
      <CardHeader className="gap-3 xl:grid-cols-[1fr_auto]">
        <div>
          <CardTitle>Jobs</CardTitle>
          <CardDescription>
            {visibleRows.length} of {jobs.length} visible from the latest page
          </CardDescription>
        </div>
        <CardAction className="grid w-full gap-2 sm:grid-cols-2 lg:grid-cols-[220px_140px_160px_150px_150px_120px] xl:w-auto">
          <div className="relative">
            <ListFilter className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Filter jobs"
              className="pl-8"
            />
          </div>
          <SelectControl
            ariaLabel="Filter by status"
            value={stateFilter}
            onChange={(value) => setColumnFilter("state", value)}
          >
            <option value="all">All states</option>
            {states.map((state) => (
              <option key={state} value={state}>
                {state}
              </option>
            ))}
          </SelectControl>
          <SelectControl
            ariaLabel="Filter by user"
            value={userFilter}
            onChange={(value) => setColumnFilter("user", value)}
          >
            <option value="all">All users</option>
            {users.map((user) => (
              <option key={user} value={user}>
                {user}
              </option>
            ))}
          </SelectControl>
          <SelectControl
            ariaLabel="Filter by GPU state"
            value={gpuFilter}
            onChange={(value) => setColumnFilter("gpu", value)}
          >
            <option value="all">All GPU states</option>
            <option value="requested">GPU requested</option>
            <option value="none">No GPU</option>
            <option value="assigned">GPU assigned</option>
            <option value="pending">GPU pending</option>
          </SelectControl>
          <div className="relative">
            <ArrowDownAZ className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
            <SelectControl
              ariaLabel="Sort jobs"
              value={sortField}
              onChange={(value) => setSortField(value as JobTableColumnId)}
              className="pl-8"
            >
              <option value="id">Sort by ID</option>
              <option value="submitted">Sort by submitted</option>
              <option value="state">Sort by status</option>
              <option value="name">Sort by name</option>
              <option value="user">Sort by user</option>
              <option value="gpu">Sort by GPU</option>
            </SelectControl>
          </div>
          <SelectControl
            ariaLabel="Sort direction"
            value={sortDirection}
            onChange={(value) => setSortDirection(value as SortDirection)}
          >
            <option value="desc">Descending</option>
            <option value="asc">Ascending</option>
          </SelectControl>
        </CardAction>
      </CardHeader>
      <CardContent>
        <div className="overflow-hidden rounded-lg border">
          <ScrollArea className="h-[520px]">
            <Table>
              <TableHeader>
                {table.getHeaderGroups().map((headerGroup) => (
                  <TableRow key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <TableHead
                        key={header.id}
                        className={header.column.id === "id" ? "w-20" : undefined}
                      >
                        {header.isPlaceholder
                          ? null
                          : flexRender(
                              header.column.columnDef.header,
                              header.getContext(),
                            )}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {visibleRows.length ? (
                  visibleRows.map((row) => (
                    <TableRow key={row.id}>
                      {row.getVisibleCells().map((cell) => (
                        <TableCell key={cell.id}>
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </TableCell>
                      ))}
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
            <GpuSlotCapsule gpus={gpus} allowed={allowed} />
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

function SelectControl({
  ariaLabel,
  value,
  onChange,
  children,
  className,
}: {
  ariaLabel: string
  value: string
  onChange: (value: string) => void
  children: ReactNode
  className?: string
}) {
  return (
    <select
      aria-label={ariaLabel}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className={cn(
        "h-8 w-full min-w-0 rounded-lg border border-input bg-background px-2.5 py-1 text-sm transition-colors outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 dark:bg-input/30",
        className,
      )}
    >
      {children}
    </select>
  )
}

function StatusBadge({ value }: { value: string }) {
  return (
    <Badge className={cn("ring-1", stateTone[value] ?? "bg-muted text-foreground")}>
      {value}
    </Badge>
  )
}

function GpuPill({ job }: { job: Job }) {
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

function formatGpuRequest(gpus?: number) {
  const count = gpus ?? 0
  return count === 1 ? "1 GPU" : `${count} GPUs`
}

function formatAssignedGpuIds(job: Job) {
  if ((job.gpus ?? 0) === 0) return "none"
  if (!Array.isArray(job.gpu_ids) || job.gpu_ids.length === 0) return "pending"
  return job.gpu_ids.map((id) => `GPU ${id}`).join(", ")
}

function jobName(job: Job) {
  return job.run_name ?? job.command ?? job.script ?? "unnamed"
}

function jobContext(job: Job) {
  return job.project ?? job.run_dir ?? ""
}

function gpuSortValue(job: Job) {
  const requested = job.gpus ?? 0
  const assigned = Array.isArray(job.gpu_ids) ? job.gpu_ids.join(",") : ""
  return `${requested.toString().padStart(4, "0")}:${assigned}`
}

function stringColumnFilter(table: TanStackTable<Job>, columnId: JobTableColumnId) {
  const value = table.getColumn(columnId)?.getFilterValue()
  return typeof value === "string" ? value : "all"
}

function exactFilter(row: Row<Job>, columnId: string, value: unknown) {
  return String(row.getValue(columnId)) === String(value)
}

function gpuStateFilter(row: Row<Job>, _columnId: string, value: unknown) {
  return matchesGpuFilter(row.original, value as GpuFilter)
}

function jobGlobalFilter(row: Row<Job>, _columnId: string, value: unknown) {
  const needle = String(value ?? "").trim().toLowerCase()
  if (!needle) return true

  const job = row.original
  return [
    job.id,
    job.state,
    job.command,
    job.script,
    job.run_name,
    job.submitted_by,
    job.project,
    job.run_dir,
    formatGpuRequest(job.gpus),
    formatAssignedGpuIds(job),
  ]
    .filter(Boolean)
    .some((candidate) => String(candidate).toLowerCase().includes(needle))
}

function matchesGpuFilter(job: Job, filter: GpuFilter) {
  const requested = (job.gpus ?? 0) > 0
  const assigned = Array.isArray(job.gpu_ids) && job.gpu_ids.length > 0

  switch (filter) {
    case "requested":
      return requested
    case "none":
      return !requested
    case "assigned":
      return requested && assigned
    case "pending":
      return requested && !assigned
    case "all":
      return true
  }
}

function uniqueSorted(values: Array<string | undefined | null>) {
  return Array.from(new Set(values.filter((value): value is string => Boolean(value)))).sort(
    (left, right) => left.localeCompare(right, undefined, { numeric: true }),
  )
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
