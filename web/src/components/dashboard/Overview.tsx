import { Activity, CheckCircle2, Cpu, Timer } from "lucide-react"

import type { DashboardData } from "@/hooks/useDashboard"
import { cn } from "@/lib/utils"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

const metricAccent = {
  emerald: "text-emerald-600 dark:text-emerald-400",
  sky: "text-sky-600 dark:text-sky-400",
  amber: "text-amber-600 dark:text-amber-400",
  zinc: "text-zinc-600 dark:text-zinc-300",
} as const

type MetricTone = keyof typeof metricAccent

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
    <Card className="rounded-lg shadow-sm">
      <CardHeader className="gap-3">
        <div className="flex items-center justify-between gap-3">
          <CardDescription className="font-medium">{label}</CardDescription>
          <span
            className={cn(
              "grid size-8 place-items-center rounded-lg bg-muted ring-1 ring-foreground/10",
              metricAccent[tone],
            )}
          >
            <Icon className="size-4" />
          </span>
        </div>
        <CardTitle className={cn("font-mono text-3xl leading-none", metricAccent[tone])}>
          {value}
        </CardTitle>
      </CardHeader>
      <CardContent className="text-sm text-muted-foreground">{detail}</CardContent>
    </Card>
  )
}
