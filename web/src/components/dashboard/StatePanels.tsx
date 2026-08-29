import { ShieldAlert } from "lucide-react"

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { TableCell, TableRow } from "@/components/ui/table"

export function EmptyRow({ columns, label }: { columns: number; label: string }) {
  return (
    <TableRow>
      <TableCell colSpan={columns}>
        <div className="flex min-h-24 items-center justify-center text-center text-sm text-muted-foreground">
          {label}
        </div>
      </TableCell>
    </TableRow>
  )
}

export function ErrorState({ message }: { message: string }) {
  return (
    <Card className="rounded-lg border-rose-200 bg-rose-50 text-rose-950 dark:border-rose-950 dark:bg-rose-950/30 dark:text-rose-100">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ShieldAlert className="size-5" />
          API unavailable
        </CardTitle>
        <CardDescription className="text-rose-800 dark:text-rose-200">
          {message}
        </CardDescription>
      </CardHeader>
    </Card>
  )
}

/** Loading skeleton that mirrors the real layout: 4 overview cards + the main panel. */
export function LoadingState() {
  return (
    <div className="flex flex-col gap-5">
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        {Array.from({ length: 4 }).map((_, index) => (
          <Card key={index} className="rounded-lg">
            <CardHeader className="gap-3">
              <Skeleton className="h-4 w-24" />
              <Skeleton className="h-8 w-20" />
            </CardHeader>
            <CardContent>
              <Skeleton className="h-4 w-32" />
            </CardContent>
          </Card>
        ))}
      </div>
      <Card className="rounded-lg">
        <CardHeader className="gap-3 border-b">
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-4 w-64" />
        </CardHeader>
        <CardContent className="space-y-2 py-4">
          {Array.from({ length: 6 }).map((_, index) => (
            <Skeleton key={index} className="h-9 w-full" />
          ))}
        </CardContent>
      </Card>
    </div>
  )
}