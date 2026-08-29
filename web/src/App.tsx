import { useState } from "react"

import type { Job } from "@/api"
import { useDashboard } from "@/hooks/useDashboard"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { DashboardHeader } from "@/components/dashboard/DashboardHeader"
import { Overview } from "@/components/dashboard/Overview"
import { JobsView } from "@/components/dashboard/JobsView"
import { GpuView } from "@/components/dashboard/GpuView"
import { ReservationsView } from "@/components/dashboard/ReservationsView"
import { StatsView } from "@/components/dashboard/StatsView"
import { JobLogDialog } from "@/components/dashboard/JobLogDialog"
import { ErrorState, LoadingState } from "@/components/dashboard/StatePanels"

function App() {
  const dashboard = useDashboard()
  const [logJob, setLogJob] = useState<Job | null>(null)

  const gpus = dashboard.data?.info.gpus ?? []

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
        <DashboardHeader
          gpuCount={gpus.length}
          jobCount={dashboard.data?.jobs.length ?? 0}
          totalJobs={dashboard.data?.stats.total_jobs}
          lastUpdated={dashboard.lastUpdated}
          connection={dashboard.connection}
          refreshing={dashboard.refreshing}
          onRefresh={() => void dashboard.refresh()}
        />

        {dashboard.error ? <ErrorState message={dashboard.error} /> : null}

        {dashboard.loading && !dashboard.data ? (
          <LoadingState />
        ) : dashboard.data ? (
          <>
            <Overview data={dashboard.data} />
            <Tabs defaultValue="jobs" className="gap-4">
              <TabsList className="grid w-full grid-cols-4 sm:w-fit">
                <TabsTrigger value="jobs">Jobs</TabsTrigger>
                <TabsTrigger value="gpus">GPUs</TabsTrigger>
                <TabsTrigger value="reservations">Reservations</TabsTrigger>
                <TabsTrigger value="stats">Stats</TabsTrigger>
              </TabsList>

              <TabsContent value="jobs">
                <JobsView
                  jobs={dashboard.data.jobs}
                  hasMore={dashboard.hasMoreJobs}
                  loadingOlder={dashboard.refreshing}
                  onLoadOlder={() => void dashboard.loadOlderJobs()}
                  onViewLogs={setLogJob}
                />
              </TabsContent>
              <TabsContent value="gpus">
                <GpuView
                  gpus={gpus}
                  allowed={dashboard.data.info.allowed_gpu_indices}
                  strategy={dashboard.data.info.gpu_allocation_strategy}
                  ignoredProcesses={dashboard.data.ignoredProcesses}
                />
              </TabsContent>
              <TabsContent value="reservations">
                <ReservationsView reservations={dashboard.data.reservations} />
              </TabsContent>
              <TabsContent value="stats">
                <StatsView stats={dashboard.data.stats} />
              </TabsContent>
            </Tabs>
          </>
        ) : null}

        <JobLogDialog job={logJob} onClose={() => setLogJob(null)} />
      </div>
    </main>
  )
}

export default App
