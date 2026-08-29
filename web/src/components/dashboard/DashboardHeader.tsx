import { useState } from "react"
import { Moon, RefreshCw, Server, Sun } from "lucide-react"

import type { ConnectionStatus } from "@/hooks/useDashboard"
import { formatClock } from "@/lib/format"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

const connectionBadge: Record<ConnectionStatus, { label: string; className: string }> = {
  live: {
    label: "Live",
    className:
      "border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200",
  },
  polling: {
    label: "Polling",
    className:
      "border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200",
  },
  connecting: {
    label: "Connecting",
    className: "bg-muted text-muted-foreground",
  },
}

const THEME_STORAGE_KEY = "gflow-theme"

function storedTheme(): "light" | "dark" {
  return document.documentElement.classList.contains("dark") ? "dark" : "light"
}

export function DashboardHeader({
  gpuCount,
  jobCount,
  totalJobs,
  lastUpdated,
  connection,
  refreshing,
  onRefresh,
}: {
  gpuCount: number
  jobCount: number
  totalJobs?: number
  lastUpdated: Date | null
  connection: ConnectionStatus
  refreshing: boolean
  onRefresh: () => void
}) {
  const badge = connectionBadge[connection]
  const [theme, setTheme] = useState<"light" | "dark">(storedTheme)

  const toggleTheme = () => {
    const next = theme === "dark" ? "light" : "dark"
    document.documentElement.classList.toggle("dark", next === "dark")
    localStorage.setItem(THEME_STORAGE_KEY, next)
    setTheme(next)
  }

  return (
    <header className="flex flex-col gap-3 border-b pb-4 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Server className="size-4" />
          <span>gflow</span>
        </div>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-foreground sm:text-3xl">
          Scheduler Console
        </h1>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <Badge variant="outline" className="bg-background">
            {gpuCount} GPUs
          </Badge>
          <Badge variant="outline" className="bg-background">
            {totalJobs != null
              ? `${totalJobs.toLocaleString()} jobs · showing ${jobCount}`
              : `${jobCount} jobs loaded`}
          </Badge>
          <Badge variant="outline" className={cn("gap-1.5", badge.className)}>
            <span
              className={cn(
                "size-1.5 rounded-full bg-current",
                connection === "live" && "animate-pulse",
              )}
            />
            {badge.label}
          </Badge>
          {lastUpdated ? (
            <span className="text-xs text-muted-foreground">
              Updated {formatClock(lastUpdated)}
            </span>
          ) : null}
        </div>
      </div>
      <div className="flex items-center gap-2">
        <Button
          onClick={toggleTheme}
          variant="outline"
          size="icon"
          aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
        >
          {theme === "dark" ? <Sun /> : <Moon />}
        </Button>
        <Button onClick={onRefresh} disabled={refreshing} size="sm" className="w-fit">
          <RefreshCw className={cn("size-4", refreshing && "animate-spin")} />
          {refreshing ? "Refreshing" : "Refresh"}
        </Button>
      </div>
    </header>
  )
}