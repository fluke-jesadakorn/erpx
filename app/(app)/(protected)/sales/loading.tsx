import { HeaderSkeleton, PanelSkeleton, Skeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <div className="flex gap-2">
        <Skeleton className="h-8 w-24" />
        <Skeleton className="h-8 w-24" />
        <Skeleton className="h-8 w-24" />
      </div>
      <PanelSkeleton rows={3} />
      <div className="space-y-2">
        {Array.from({ length: 5 }).map((_, i) => (
          <PanelSkeleton key={i} rows={2} />
        ))}
      </div>
    </main>
  );
}