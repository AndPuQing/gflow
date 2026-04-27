export type ApiResult<T> = {
  data: T | null
  error: string | null
  loading: boolean
}

export type SchedulerInfo = {
  gpus?: GpuInfo[]
  allowed_gpu_indices?: number[] | null
  gpu_allocation_strategy?: string
}

export type GpuInfo = {
  uuid: string
  index: number
  available: boolean
  reason?: string | null
}

export type Job = {
  id: number
  state: string
  command?: string | null
  script?: string | null
  submitted_by?: string
  submitted_at?: ApiTime | null
  started_at?: ApiTime | null
  finished_at?: ApiTime | null
  run_name?: string | null
  project?: string | null
  run_dir?: string
  gpus?: number
  gpu_ids?: number[] | null
  priority?: number
  group_id?: string | null
  task_id?: number | null
  reason?: unknown
}

export type UsageStats = {
  total_jobs: number
  completed_jobs: number
  failed_jobs: number
  cancelled_jobs: number
  timeout_jobs: number
  running_jobs: number
  queued_jobs: number
  avg_wait_secs?: number | null
  avg_runtime_secs?: number | null
  total_gpu_hours: number
  jobs_with_gpus: number
  avg_gpus_per_job: number
  peak_gpu_usage: number
  success_rate: number
  top_jobs: Array<{
    id: number
    name?: string | null
    runtime_secs: number
    gpus: number
  }>
}

export type Reservation = {
  id: number
  user: string
  gpu_spec: unknown
  start_time: ApiTime
  duration: ApiDuration
  status: string
  created_at: ApiTime
  cancelled_at?: ApiTime | null
}

export type IgnoredGpuProcess = {
  gpu_index: number
  pid: number
}

export type ApiTime =
  | string
  | number
  | {
      secs_since_epoch?: number
      nanos_since_epoch?: number
      seconds?: number
      nanos?: number
    }

export type ApiDuration =
  | number
  | {
      secs?: number
      nanos?: number
    }

export async function fetchJson<T>(path: string): Promise<T> {
  const response = await fetch(path, {
    headers: { accept: "application/json" },
  })

  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`)
  }

  return response.json() as Promise<T>
}

export function unwrapError(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
