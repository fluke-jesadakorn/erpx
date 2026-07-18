import { HeaderSkeleton, KpiSkeleton, PanelSkeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <PanelSkeleton rows={3} />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <KpiSkeleton />
        <KpiSkeleton />
        <KpiSkeleton />
      </div>
      <PanelSkeleton rows={4} />
      <div className="space-y-2">
        {Array.from({ length: 3 }).map((_, i) => (
          <PanelSkeleton key={i} rows={2} />
        ))}
      </div>
    </main>
  );
}