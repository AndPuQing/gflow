import { useCallback, useEffect, useRef, useState } from "react"
import { ArrowDown, Check, Copy, RefreshCw } from "lucide-react"

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
import { cn } from "@/lib/utils"

type LogState = {
  content: string
  truncated: boolean
  size: number
  fetchedAt: Date | null
}

/** States whose log can still change while the dialog is open. */
function isLogLive(state: string) {
  return state === "Running" || state === "Queued"
}

export function JobLogDialog({ job, onClose }: { job: Job | null; onClose: () => void }) {
  const [log, setLog] = useState<LogState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [copied, setCopied] = useState(false)
  const [atBottom, setAtBottom] = useState(true)
  const hostRef = useRef<HTMLDivElement | null>(null)

  const viewport = () =>
    hostRef.current?.querySelector<HTMLElement>("[data-slot=scroll-area-viewport]")

  const jumpToBottom = useCallback((stick = true) => {
    const el = viewport()
    if (el) el.scrollTop = el.scrollHeight
    if (stick) setAtBottom(true)
  }, [])

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

  // Fetch on open / job change, then keep tailing while the job is active.
  const live = job !== null && isLogLive(job.state)
  useEffect(() => {
    if (!job) {
      setLog(null)
      setError(null)
      setLoading(false)
      setAtBottom(true)
      return
    }

    void load(job)
    if (!isLogLive(job.state)) return
    const timer = window.setInterval(() => void load(job), LOG_REFRESH_INTERVAL_MS)
    return () => window.clearInterval(timer)
  }, [job, live, load])

  // Follow the tail: pin to the bottom unless the user scrolled up.
  useEffect(() => {
    const el = viewport()
    if (!el) return
    const onScroll = () =>
      setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 80)
    el.addEventListener("scroll", onScroll)
    return () => el.removeEventListener("scroll", onScroll)
  }, [job, log?.content])

  useEffect(() => {
    if (log?.content && atBottom) {
      const el = viewport()
      if (el) el.scrollTop = el.scrollHeight
    }
  }, [log?.content, atBottom])

  const copyLog = async () => {
    if (!log?.content) return
    try {
      await navigator.clipboard.writeText(log.content)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // Clipboard unavailable (e.g. non-secure context); ignore.
    }
  }

  return (
    <Dialog open={job !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[85vh] flex-col gap-3 max-w-[min(90vw,896px)] sm:max-w-[896px]">
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

        <div ref={hostRef} className="relative">
          <ScrollArea className="h-[55vh] min-h-[320px] rounded-lg border bg-zinc-950 text-zinc-100">
            <pre className="p-4 font-mono text-[13px] leading-relaxed whitespace-pre-wrap break-words">
              {log?.content ? log.content : loading ? "Loading…" : "No log output yet."}
            </pre>
          </ScrollArea>
          {log?.content && !atBottom ? (
            <Button
              size="sm"
              onClick={() => jumpToBottom()}
              className="absolute right-3 bottom-3 shadow-md"
            >
              <ArrowDown className="size-3.5" />
              Latest
            </Button>
          ) : null}
        </div>

        <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
          <span className="truncate">
            {log ? `${log.size.toLocaleString()} bytes on disk` : "—"}
          </span>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={copyLog}
              disabled={!log?.content || loading}
            >
              {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              {copied ? "Copied" : "Copy"}
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={loading || job === null}
              onClick={() => job && void load(job)}
            >
              <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
              Refresh
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}