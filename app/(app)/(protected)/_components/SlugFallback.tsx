import { HeaderSkeleton, KpiSkeleton, PanelSkeleton, SkeletonGrid } from '@/components/ui/Loading';

export function SlugFallback() {
  return (
    <div className="animate-fade-in space-y-6" aria-busy="true">
      <HeaderSkeleton />
      <SkeletonGrid cols={4} count={4} renderItem={() => <KpiSkeleton />} />
      <PanelSkeleton rows={5} />
    </div>
  );
}
