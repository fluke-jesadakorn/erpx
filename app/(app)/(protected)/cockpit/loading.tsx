import { HeaderSkeleton, KpiSkeleton, PanelSkeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <KpiSkeleton />
        <KpiSkeleton />
        <KpiSkeleton />
        <KpiSkeleton />
      </div>
      <PanelSkeleton rows={6} />
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <PanelSkeleton rows={4} />
        <PanelSkeleton rows={4} />
        <PanelSkeleton rows={4} />
      </div>
    </main>
  );
}