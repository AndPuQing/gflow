import type { Reservation } from "@/api"
import { formatDuration, formatGpuSpec, formatTime } from "@/lib/format"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { EmptyRow } from "@/components/dashboard/StatePanels"
import { StatusBadge } from "@/components/dashboard/StatusBadge"

export function ReservationsView({ reservations }: { reservations: Reservation[] }) {
  return (
    <Card className="rounded-lg">
      <CardHeader>
        <CardTitle>Reservations</CardTitle>
        <CardDescription>{reservations.length} reservations</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-hidden rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>User</TableHead>
                <TableHead>GPU Spec</TableHead>
                <TableHead>Start</TableHead>
                <TableHead>Duration</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {reservations.length ? (
                reservations.map((reservation) => (
                  <TableRow key={reservation.id}>
                    <TableCell className="font-mono text-xs">
                      {reservation.id}
                    </TableCell>
                    <TableCell>
                      <StatusBadge value={reservation.status} />
                    </TableCell>
                    <TableCell>{reservation.user}</TableCell>
                    <TableCell>{formatGpuSpec(reservation.gpu_spec)}</TableCell>
                    <TableCell>{formatTime(reservation.start_time)}</TableCell>
                    <TableCell>{formatDuration(reservation.duration)}</TableCell>
                  </TableRow>
                ))
              ) : (
                <EmptyRow columns={6} label="No reservations" />
              )}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  )
}
