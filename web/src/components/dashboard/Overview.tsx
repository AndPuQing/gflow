import { Activity, CheckCircle2, Cpu, Timer } from "lucide-react"

import type { DashboardData } from "@/hooks/useDashboard"
import { cn } from "@/lib/utils"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

const metricTone = {
  emerald:
    "border-emerald-200 bg-emerald-50/80 text-emerald-950 dark:border-emerald-950 dark:bg-emerald-950/25 dark:text-emerald-100",
  sky: "border-sky-200 bg-sky-50/80 text-sky-950 dark:border-sky-950 dark:bg-sky-950/25 dark:text-sky-100",
  amber:
    "border-amber-200 bg-amber-50/80 text-amber-950 dark:border-amber-950 dark:bg-amber-950/25 dark:text-amber-100",
  zinc: "border-zinc-200 bg-zinc-50/80 text-zinc-950 dark:border-zinc-800 dark:bg-zinc-900/60 dark:text-zinc-100",
} as const

type MetricTone = keyof typeof metricTone

export function Overview({ data }: { data: DashboardData }) {
  const available = data.info.gpus?.filter((gpu) => gpu.available).length ?? 0
  const total = data.info.gpus?.length ?? 0
  const busy = Math.max(total - available, 0)
  const completed = data.stats.completed_jobs
  const problemJobs =
    data.stats.failed_jobs + data.stats.cancelled_jobs + data.stats.timeout_jobs

  return (
    <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
      <MetricCard
        icon={Activity}
        label="Running"
        value={data.stats.running_jobs}
        detail={`${data.stats.queued_jobs} queued for scheduling`}
        tone="emerald"
      />
      <MetricCard
        icon={CheckCircle2}
        label="Success"
        value={`${data.stats.success_rate.toFixed(1)}%`}
        detail={`${completed} completed · ${problemJobs} need review`}
        tone="sky"
      />
      <MetricCard
        icon={Cpu}
        label="GPU Slots"
        value={`${available}/${total}`}
        detail={`${busy} busy · ${data.stats.peak_gpu_usage} peak request`}
        tone="amber"
      />
      <MetricCard
        icon={Timer}
        label="GPU Hours"
        value={data.stats.total_gpu_hours.toFixed(1)}
        detail={`${data.stats.jobs_with_gpus} GPU jobs`}
        tone="zinc"
      />
    </section>
  )
}

function MetricCard({
  icon: Icon,
  label,
  value,
  detail,
  tone,
}: {
  icon: typeof Activity
  label: string
  value: string | number
  detail: string
  tone: MetricTone
}) {
  return (
    <Card className={cn("rounded-lg border shadow-sm", metricTone[tone])}>
      <CardHeader className="gap-3">
        <div className="flex items-center justify-between gap-3">
          <CardDescription className="font-medium text-current/70">
            {label}
          </CardDescription>
          <span className="grid size-8 place-items-center rounded-lg bg-background/80 ring-1 ring-current/10">
            <Icon className="size-4" />
          </span>
        </div>
        <CardTitle className="font-mono text-3xl leading-none">{value}</CardTitle>
      </CardHeader>
      <CardContent className="text-sm text-current/65">{detail}</CardContent>
    </Card>
  )
}
