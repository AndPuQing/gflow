import { useCallback, useEffect, useRef, useState } from "react"

import {
  fetchJson,
  type IgnoredGpuProcess,
  type Job,
  type Reservation,
  type SchedulerInfo,
  type UsageStats,
  unwrapError,
} from "@/api"

export type DashboardData = {
  info: SchedulerInfo
  jobs: Job[]
  stats: UsageStats
  reservations: Reservation[]
  ignoredProcesses: IgnoredGpuProcess[]
}

export type ConnectionStatus = "connecting" | "live" | "polling"

/** Fallback polling interval used when the SSE stream is unavailable. */
const POLL_INTERVAL_MS = 5000
/** How often to retry establishing the SSE stream while polling. */
const SSE_RETRY_INTERVAL_MS = 15000
/** Burst window for coalescing event-driven refreshes. */
const EVENT_REFRESH_DELAY_MS = 350
/** Interval for refreshing an open job log view. */
export const LOG_REFRESH_INTERVAL_MS = 4000

/**
 * Named SSE events emitted by the daemon's `/events` endpoint. Every event is
 * treated as a "re-sync" hint: the payload says what changed, the dashboard
 * responds by re-fetching the REST endpoints.
 */
export const DASHBOARD_EVENT_NAMES = [
  "connected",
  "job_state_changed",
  "job_submitted",
  "job_updated",
  "job_completed",
  "gpu_availability_changed",
  "manual_gpu_override_changed",
  "memory_availability_changed",
  "job_timed_out",
  "zombie_job_detected",
  "reservation_created",
  "reservation_cancelled",
  "daemon_started",
] as const

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

export type DashboardState = {
  data: DashboardData | null
  error: string | null
  /** True until the first fetch settles. */
  loading: boolean
  /** True while any fetch is in flight. */
  refreshing: boolean
  connection: ConnectionStatus
  lastUpdated: Date | null
  refresh: () => Promise<void>
}

/**
 * Dashboard data with live updates: an EventSource on `/events` triggers
 * debounced re-fetches; if the stream fails, the hook falls back to polling
 * and keeps retrying the stream in the background.
 */
export function useDashboard(): DashboardState {
  const [data, setData] = useState<DashboardData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [connection, setConnection] = useState<ConnectionStatus>("connecting")
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null)

  const inFlight = useRef(false)
  const debounceTimer = useRef<number | null>(null)

  const refresh = useCallback(async () => {
    if (inFlight.current) return
    inFlight.current = true
    setRefreshing(true)
    try {
      const dashboard = await fetchDashboard()
      setData(dashboard)
      setError(null)
      setLastUpdated(new Date())
    } catch (err) {
      setError(unwrapError(err))
    } finally {
      inFlight.current = false
      setRefreshing(false)
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    let disposed = false
    let source: EventSource | null = null
    let pollTimer: number | null = null
    let retryTimer: number | null = null

    const clearPolling = () => {
      if (pollTimer !== null) {
        window.clearInterval(pollTimer)
        pollTimer = null
      }
    }

    const scheduleRefresh = () => {
      if (debounceTimer.current !== null) return
      debounceTimer.current = window.setTimeout(() => {
        debounceTimer.current = null
        void refresh()
      }, EVENT_REFRESH_DELAY_MS)
    }

    const startPolling = () => {
      if (pollTimer !== null) return
      setConnection("polling")
      pollTimer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS)
    }

    const connect = () => {
      if (disposed || source) return
      setConnection("connecting")
      const events = new EventSource("/events")
      source = events

      events.onopen = () => {
        if (disposed) return
        setConnection("live")
        clearPolling()
        void refresh()
      }
      events.onerror = () => {
        events.close()
        if (source === events) source = null
        if (disposed) return
        startPolling()
      }
      for (const name of DASHBOARD_EVENT_NAMES) {
        events.addEventListener(name, scheduleRefresh)
      }
    }

    void refresh()
    connect()
    // Keep retrying the stream while polling so we return to live updates.
    retryTimer = window.setInterval(connect, SSE_RETRY_INTERVAL_MS)

    return () => {
      disposed = true
      source?.close()
      source = null
      clearPolling()
      if (retryTimer !== null) window.clearInterval(retryTimer)
      if (debounceTimer.current !== null) {
        window.clearTimeout(debounceTimer.current)
        debounceTimer.current = null
      }
    }
  }, [refresh])

  return { data, error, loading, refreshing, connection, lastUpdated, refresh }
}
