import { useMemo, useState, type ReactNode } from "react"
import {
  type ColumnDef,
  type ColumnFiltersState,
  flexRender,
  type SortingState,
  useTable,
} from "@tanstack/react-table"
import {
  ArrowDown,
  ArrowUp,
  FileText,
  History,
  ListFilter,
  X,
} from "lucide-react"

import type { Job } from "@/api"
import { formatRuntime, formatTime, toDate } from "@/lib/format"
import {
  exactFilter,
  jobTableFeatures,
  type GpuFilter,
  gpuSortValue,
  gpuStateFilter,
  jobContext,
  jobGlobalFilter,
  jobName,
  type JobTableColumnId,
  type JobTableFeatures,
  stringColumnFilter,
  uniqueSorted,
} from "@/lib/jobs"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select } from "@/components/ui/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { GpuPill } from "@/components/dashboard/GpuPill"
import { EmptyRow } from "@/components/dashboard/StatePanels"
import { StatusBadge } from "@/components/dashboard/StatusBadge"
import { SummaryPill } from "@/components/dashboard/SummaryPill"

const DEFAULT_SORTING: SortingState = [{ id: "id", desc: true }]

function jobRuntimeSeconds(job: Job): number | null {
  const start = toDate(job.started_at)
  if (!start) return null
  const end = toDate(job.finished_at) ?? new Date()
  return Math.max(0, (end.getTime() - start.getTime()) / 1000)
}

export function JobsView({
  jobs,
  hasMore,
  loadingOlder,
  onLoadOlder,
  onViewLogs,
}: {
  jobs: Job[]
  hasMore: boolean
  loadingOlder: boolean
  onLoadOlder: () => void
  onViewLogs: (job: Job) => void
}) {
  const [query, setQuery] = useState("")
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([])
  const [sorting, setSorting] = useState<SortingState>(DEFAULT_SORTING)

  const states = useMemo(() => uniqueSorted(jobs.map((job) => job.state)), [jobs])
  const users = useMemo(
    () => uniqueSorted(jobs.map((job) => job.submitted_by ?? "unknown")),
    [jobs],
  )

  const columns = useMemo<ColumnDef<JobTableFeatures, Job>[]>(
    () => [
      {
        accessorKey: "id",
        header: "ID",
        cell: ({ row }) => <span className="font-mono text-xs">{row.original.id}</span>,
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
      {
        id: "runtime",
        accessorFn: jobRuntimeSeconds,
        header: "Runtime",
        cell: ({ row }) => {
          const display = formatRuntime(jobRuntimeSeconds(row.original))
          return (
            <span
              className={cn(
                "font-mono text-xs",
                display == null && "text-muted-foreground",
              )}
            >
              {display ?? "—"}
            </span>
          )
        },
        sortingFn: "basic",
      },
      {
        id: "logs",
        header: () => null,
        enableSorting: false,
        cell: ({ row }) => (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`View logs for job ${row.original.id}`}
            onClick={(event) => {
              event.stopPropagation()
              onViewLogs(row.original)
            }}
          >
            <FileText />
          </Button>
        ),
      },
    ],
    [onViewLogs],
  )

  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useTable({
    features: jobTableFeatures,
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
    globalFilterFn: jobGlobalFilter,
  })

  const visibleRows = table.getRowModel().rows
  const stateFilter = stringColumnFilter(table, "state")
  const userFilter = stringColumnFilter(table, "user")
  const gpuFilter = stringColumnFilter(table, "gpu") as GpuFilter
  const activeSort = sorting[0]
  const sortField = (activeSort?.id ?? "id") as JobTableColumnId
  const sortIsDesc = activeSort?.desc !== false
  const runningJobs = jobs.filter((job) => job.state === "Running").length
  const queuedJobs = jobs.filter((job) => job.state === "Queued").length
  const gpuJobs = jobs.filter((job) => (job.gpus ?? 0) > 0).length
  const hasControlsActive =
    query.trim().length > 0 ||
    stateFilter !== "all" ||
    userFilter !== "all" ||
    gpuFilter !== "all" ||
    sortField !== "id" ||
    !sortIsDesc

  const setColumnFilter = (columnId: JobTableColumnId, value: string) => {
    table.getColumn(columnId)?.setFilterValue(value === "all" ? undefined : value)
  }

  const resetControls = () => {
    setQuery("")
    setColumnFilters([])
    setSorting(DEFAULT_SORTING)
  }

  return (
    <Card className="rounded-lg">
      <CardHeader className="gap-4 border-b">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <CardTitle>Jobs</CardTitle>
            <CardDescription>
              {visibleRows.length} of {jobs.length} visible · click a row for logs
            </CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <SummaryPill label="Running" value={runningJobs} tone="emerald" />
            <SummaryPill label="Queued" value={queuedJobs} tone="sky" />
            <SummaryPill label="GPU jobs" value={gpuJobs} tone="amber" />
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-[minmax(220px,1.2fr)_140px_160px_150px_auto]">
          <div className="relative">
            <ListFilter className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Filter jobs"
              className="pl-8"
            />
          </div>
          <Select
            aria-label="Filter by status"
            value={stateFilter}
            onChange={(event) => setColumnFilter("state", event.target.value)}
          >
            <option value="all">All states</option>
            {states.map((state) => (
              <option key={state} value={state}>
                {state}
              </option>
            ))}
          </Select>
          <Select
            aria-label="Filter by user"
            value={userFilter}
            onChange={(event) => setColumnFilter("user", event.target.value)}
          >
            <option value="all">All users</option>
            {users.map((user) => (
              <option key={user} value={user}>
                {user}
              </option>
            ))}
          </Select>
          <Select
            aria-label="Filter by GPU state"
            value={gpuFilter}
            onChange={(event) => setColumnFilter("gpu", event.target.value)}
          >
            <option value="all">All GPU states</option>
            <option value="requested">GPU requested</option>
            <option value="none">No GPU</option>
            <option value="assigned">GPU assigned</option>
            <option value="pending">GPU pending</option>
          </Select>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={resetControls}
            disabled={!hasControlsActive}
            className="h-8 justify-start lg:justify-center"
          >
            <X className="size-4" />
            Reset
          </Button>
        </div>
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
                        className={cn(
                          "sticky top-0 z-10 bg-muted/95 backdrop-blur",
                          header.column.id === "id" && "w-20",
                          header.column.id === "logs" && "w-12 text-right",
                        )}
                      >
                        {header.isPlaceholder ? null : (
                          <HeaderCell
                            content={flexRender(
                              header.column.columnDef.header,
                              header.getContext(),
                            )}
                            canSort={header.column.getCanSort()}
                            isSorted={header.column.getIsSorted()}
                            onToggleSort={header.column.getToggleSortingHandler()}
                          />
                        )}
                      </TableHead>
                    ))}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {visibleRows.length ? (
                  visibleRows.map((row) => (
                    <TableRow
                      key={row.id}
                      className="cursor-pointer"
                      onClick={() => onViewLogs(row.original)}
                    >
                      {row.getAllCells().map((cell) => (
                        <TableCell key={cell.id} className={cn(cell.column.id === "logs" && "text-right")}>
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))
                ) : (
                  <EmptyRow columns={7} label="No jobs match the current filter" />
                )}
              </TableBody>
            </Table>
          </ScrollArea>
        </div>
        {hasMore ? (
          <div className="mt-3 flex justify-center">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={onLoadOlder}
              disabled={loadingOlder}
            >
              <History className="size-4" />
              {loadingOlder ? "Loading…" : "Load earlier jobs"}
            </Button>
          </div>
        ) : null}
      </CardContent>
    </Card>
  )
}

function HeaderCell({
  content,
  canSort,
  isSorted,
  onToggleSort,
}: {
  content: ReactNode
  canSort: boolean
  isSorted: false | "asc" | "desc"
  onToggleSort?: (event: unknown) => void
}) {
  if (!canSort || !onToggleSort) return <>{content}</>
  const Icon = isSorted === "asc" ? ArrowUp : isSorted === "desc" ? ArrowDown : null
  return (
    <button
      type="button"
      onClick={onToggleSort}
      title="Sort"
      className="inline-flex items-center gap-1 rounded-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      {content}
      <span className={cn("inline-flex", isSorted ? "text-foreground" : "opacity-40")}>
        {Icon ? (
          <Icon className="size-3.5" />
        ) : (
          <ArrowDown className="size-3.5" aria-hidden />
        )}
      </span>
    </button>
  )
}