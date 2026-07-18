import { HeaderSkeleton, PanelSkeleton, Skeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <PanelSkeleton rows={1} />
      <div className="space-y-3">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="flex gap-3">
            <Skeleton className="h-8 w-8" rounded="full" />
            <Skeleton className={i % 2 === 0 ? 'h-16 w-2/3' : 'h-16 w-1/2'} />
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        <Skeleton className="h-9 w-24" />
        <Skeleton className="h-9 w-32" />
      </div>
    </main>
  );
}