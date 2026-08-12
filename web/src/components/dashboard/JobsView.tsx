import { useMemo, useState } from "react"
import {
  type ColumnDef,
  type ColumnFiltersState,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getSortedRowModel,
  type SortingState,
  useReactTable,
} from "@tanstack/react-table"
import { ArrowDownAZ, FileText, ListFilter, X } from "lucide-react"

import type { Job } from "@/api"
import { formatTime, toDate } from "@/lib/format"
import {
  exactFilter,
  type GpuFilter,
  gpuSortValue,
  gpuStateFilter,
  jobContext,
  jobGlobalFilter,
  jobName,
  type JobTableColumnId,
  type SortDirection,
  stringColumnFilter,
  uniqueSorted,
} from "@/lib/jobs"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { GpuPill } from "@/components/dashboard/GpuPill"
import { SelectControl } from "@/components/dashboard/SelectControl"
import { EmptyRow } from "@/components/dashboard/StatePanels"
import { StatusBadge } from "@/components/dashboard/StatusBadge"
import { SummaryPill } from "@/components/dashboard/SummaryPill"

export function JobsView({ jobs, onViewLogs }: { jobs: Job[]; onViewLogs: (job: Job) => void }) {
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
      {
        id: "scheduled",
        accessorFn: (job) => (job.scheduled_at ? (toDate(job.scheduled_at)?.valueOf() ?? 0) : 0),
        header: "Starts at",
        cell: ({ row }) =>
          row.original.scheduled_at ? formatTime(row.original.scheduled_at) : "—",
        sortingFn: "basic",
      },
      {
        id: "actions",
        header: "Logs",
        enableSorting: false,
        cell: ({ row }) => (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={`View logs for job ${row.original.id}`}
            onClick={() => onViewLogs(row.original)}
          >
            <FileText />
          </Button>
        ),
      },
    ],
    [onViewLogs],
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
  const runningJobs = jobs.filter((job) => job.state === "Running").length
  const queuedJobs = jobs.filter((job) => job.state === "Queued").length
  const gpuJobs = jobs.filter((job) => (job.gpus ?? 0) > 0).length
  const hasControlsActive =
    query.trim().length > 0 ||
    stateFilter !== "all" ||
    userFilter !== "all" ||
    gpuFilter !== "all" ||
    sortField !== "id" ||
    sortDirection !== "desc"

  const setColumnFilter = (columnId: JobTableColumnId, value: string) => {
    table.getColumn(columnId)?.setFilterValue(value === "all" ? undefined : value)
  }

  const setSortField = (columnId: JobTableColumnId) => {
    setSorting([{ id: columnId, desc: sortDirection === "desc" }])
  }

  const setSortDirection = (direction: SortDirection) => {
    setSorting([{ id: sortField, desc: direction === "desc" }])
  }

  const resetControls = () => {
    setQuery("")
    setColumnFilters([])
    setSorting([{ id: "id", desc: true }])
  }

  return (
    <Card className="rounded-lg">
      <CardHeader className="gap-4 border-b">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <CardTitle>Jobs</CardTitle>
            <CardDescription>
              {visibleRows.length} of {jobs.length} visible from the latest page
            </CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <SummaryPill label="Running" value={runningJobs} tone="emerald" />
            <SummaryPill label="Queued" value={queuedJobs} tone="sky" />
            <SummaryPill label="GPU jobs" value={gpuJobs} tone="amber" />
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-[minmax(220px,1.2fr)_140px_160px_150px_150px_120px_auto]">
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
                        )}
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
                  <EmptyRow columns={7} label="No jobs match the current filter" />
                )}
              </TableBody>
            </Table>
          </ScrollArea>
        </div>
      </CardContent>
    </Card>
  )
}
