import { HeaderSkeleton, SkeletonGrid, TileSkeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <div className="animate-fade-in">
      <HeaderSkeleton />
      <SkeletonGrid cols={4} count={8} renderItem={() => <TileSkeleton />} />
    </div>
  );
}
