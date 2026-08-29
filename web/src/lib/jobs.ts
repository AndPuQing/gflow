import {
  columnFilteringFeature,
  createFilteredRowModel,
  createSortedRowModel,
  globalFilteringFeature,
  rowSortingFeature,
  tableFeatures,
  type Row,
  type Table,
} from "@tanstack/react-table"

import type { Job } from "@/api"
import { formatAssignedGpuIds, formatGpuRequest } from "@/lib/format"

/**
 * Feature set for the jobs table. v9 of @tanstack/react-table requires every
 * non-core feature and row model to be registered explicitly. The jobs table
 * uses column filtering, global filtering, and sorting, so those (plus the
 * filtered/sorted row models and `columnFilteringFeature`, which global
 * filtering depends on) are registered here.
 */
export const jobTableFeatures = tableFeatures({
  columnFilteringFeature,
  globalFilteringFeature,
  rowSortingFeature,
  filteredRowModel: createFilteredRowModel(),
  sortedRowModel: createSortedRowModel(),
})

export type JobTableFeatures = typeof jobTableFeatures
type JobTable = Table<JobTableFeatures, Job>
type JobRow = Row<JobTableFeatures, Job>

export type JobTableColumnId =
  | "id"
  | "state"
  | "name"
  | "user"
  | "gpu"
  | "submitted"
export type GpuFilter = "all" | "requested" | "none" | "assigned" | "pending"
export type SortDirection = "asc" | "desc"

export function jobName(job: Job) {
  return job.run_name ?? job.command ?? job.script ?? "unnamed"
}

export function jobContext(job: Job) {
  return job.project ?? job.run_dir ?? ""
}

export function gpuSortValue(job: Job) {
  const requested = job.gpus ?? 0
  const assigned = Array.isArray(job.gpu_ids) ? job.gpu_ids.join(",") : ""
  return `${requested.toString().padStart(4, "0")}:${assigned}`
}

export function stringColumnFilter(table: JobTable, columnId: JobTableColumnId) {
  const value = table.getColumn(columnId)?.getFilterValue()
  return typeof value === "string" ? value : "all"
}

export function exactFilter(row: JobRow, columnId: string, value: unknown) {
  return String(row.getValue(columnId)) === String(value)
}

export function gpuStateFilter(row: JobRow, _columnId: string, value: unknown) {
  return matchesGpuFilter(row.original, value as GpuFilter)
}

export function jobGlobalFilter(row: JobRow, _columnId: string, value: unknown) {
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

export function matchesGpuFilter(job: Job, filter: GpuFilter) {
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

export function uniqueSorted(values: Array<string | undefined | null>) {
  return Array.from(new Set(values.filter((value): value is string => Boolean(value)))).sort(
    (left, right) => left.localeCompare(right, undefined, { numeric: true }),
  )
}
