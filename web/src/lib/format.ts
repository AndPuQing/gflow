import type { ApiDuration, ApiTime, Job } from "@/api"

export function formatGpuRequest(gpus?: number) {
  const count = gpus ?? 0
  return count === 1 ? "1 GPU" : `${count} GPUs`
}

export function formatAssignedGpuIds(job: Job) {
  if ((job.gpus ?? 0) === 0) return "none"
  if (!Array.isArray(job.gpu_ids) || job.gpu_ids.length === 0) return "pending"
  return job.gpu_ids.map((id) => `GPU ${id}`).join(", ")
}

export function formatGpuSpec(value: unknown) {
  if (typeof value === "number" || typeof value === "string") return String(value)
  if (!value || typeof value !== "object") return "unknown"

  const record = value as Record<string, unknown>
  if (typeof record.count === "number") return `${record.count} GPUs`
  if (Array.isArray(record.indices)) return `GPU ${record.indices.join(", ")}`
  if (typeof record.Count === "number") return `${record.Count} GPUs`
  if (Array.isArray(record.Indices)) return `GPU ${record.Indices.join(", ")}`

  return JSON.stringify(value)
}

export function formatTime(value?: ApiTime | null) {
  const date = toDate(value)
  if (!date) return "not set"
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date)
}

export function formatClock(value: Date) {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(value)
}

export function formatDuration(value?: ApiDuration | null) {
  if (value == null) return "not set"
  const seconds =
    typeof value === "number" ? value : Number(value.secs ?? 0) + Number(value.nanos ?? 0) / 1e9
  return formatSeconds(seconds)
}

export function formatSeconds(value?: number | null) {
  if (value == null || Number.isNaN(value)) return "not set"
  if (value < 60) return `${value.toFixed(1)}s`
  if (value < 3600) return `${(value / 60).toFixed(1)}m`
  return `${(value / 3600).toFixed(1)}h`
}

export function toDate(value?: ApiTime | null) {
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

// eslint-disable-next-line no-control-regex
const ANSI_PATTERN = /\u001b\[[0-9;?]*[a-zA-Z]|\u001b\][^\u0007]*(\u0007|\u001b\\)/g

/** Drop ANSI escape sequences so raw terminal output renders as plain text. */
export function stripAnsi(value: string) {
  return value.replace(ANSI_PATTERN, "")
}
