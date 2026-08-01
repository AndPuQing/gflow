import { useCallback, useEffect, useRef, useState } from "react"
import { RefreshCw } from "lucide-react"

import { fetchJobLogContent, type Job, unwrapError } from "@/api"
import { LOG_REFRESH_INTERVAL_MS } from "@/hooks/useDashboard"
import { jobName } from "@/lib/jobs"
import { formatClock, stripAnsi } from "@/lib/format"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import { StatusBadge } from "@/components/dashboard/StatusBadge"

type LogState = {
  content: string
  truncated: boolean
  size: number
  fetchedAt: Date | null
}

export function JobLogDialog({ job, onClose }: { job: Job | null; onClose: () => void }) {
  const [log, setLog] = useState<LogState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const scrollRef = useRef<HTMLDivElement | null>(null)

  const load = useCallback(async (target: Job) => {
    setLoading(true)
    try {
      const result = await fetchJobLogContent(target.id)
      setLog({
        content: stripAnsi(result.content),
        truncated: result.truncated,
        size: result.size,
        fetchedAt: new Date(),
      })
      setError(null)
    } catch (err) {
      setError(unwrapError(err))
    } finally {
      setLoading(false)
    }
  }, [])

  // Fetch on open / job change, then keep tailing while the dialog is open.
  useEffect(() => {
    if (!job) {
      setLog(null)
      setError(null)
      setLoading(false)
      return
    }

    void load(job)
    const timer = window.setInterval(() => void load(job), LOG_REFRESH_INTERVAL_MS)
    return () => window.clearInterval(timer)
  }, [job, load])

  // Follow the log: stay pinned to the bottom unless the user scrolled up.
  useEffect(() => {
    const viewport = scrollRef.current?.querySelector<HTMLElement>("[data-slot=scroll-area-viewport]")
      ?? scrollRef.current?.firstElementChild
    if (!viewport) return
    const nearBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 80
    if (nearBottom) viewport.scrollTop = viewport.scrollHeight
  }, [log?.content])

  return (
    <Dialog open={job !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-w-3xl flex-col gap-3">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 pr-8">
            {job ? <StatusBadge value={job.state} /> : null}
            <span className="truncate">
              {job ? `Job ${job.id} · ${jobName(job)}` : "Job log"}
            </span>
          </DialogTitle>
          <DialogDescription>
            {job?.submitted_by ? `${job.submitted_by} · ` : ""}
            {log?.fetchedAt ? `updated ${formatClock(log.fetchedAt)}` : "loading log…"}
            {log?.truncated ? " · showing the tail of the log" : ""}
          </DialogDescription>
        </DialogHeader>

        {error ? (
          <div className="rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-200">
            {error}
          </div>
        ) : null}

        <ScrollArea ref={scrollRef} className="h-[420px] rounded-lg border bg-zinc-950 text-zinc-100">
          <pre className="p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap break-all">
            {log?.content ? log.content : loading ? "Loading…" : "No log output yet."}
          </pre>
        </ScrollArea>

        <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span>
            {log ? `${log.size.toLocaleString()} bytes on disk` : "—"}
          </span>
          <Button
            variant="outline"
            size="sm"
            disabled={loading || job === null}
            onClick={() => job && void load(job)}
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} />
            Refresh
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
