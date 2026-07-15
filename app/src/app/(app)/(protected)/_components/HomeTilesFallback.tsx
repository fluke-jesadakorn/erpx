import { HeaderSkeleton, SkeletonGrid, TileSkeleton } from '@/components/ui/Loading';

export function HomeTilesFallback() {
  return (
    <div className="animate-fade-in" aria-busy="true">
      <HeaderSkeleton />
      <SkeletonGrid cols={4} count={8} renderItem={() => <TileSkeleton />} />
    </div>
  );
}
