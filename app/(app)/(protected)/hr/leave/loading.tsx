import { HeaderSkeleton, PanelSkeleton, Skeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <section className="space-y-4">
        <Skeleton className="h-4 w-32" />
        <div className="space-y-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <PanelSkeleton key={i} rows={3} />
          ))}
        </div>
      </section>
      <section className="space-y-4">
        <Skeleton className="h-4 w-32" />
        <div className="space-y-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <PanelSkeleton key={i} rows={3} />
          ))}
        </div>
      </section>
    </main>
  );
}