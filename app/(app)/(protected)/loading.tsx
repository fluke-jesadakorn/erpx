import { HeaderSkeleton, SkeletonGrid, TileSkeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <SkeletonGrid cols={3} count={6} renderItem={() => <TileSkeleton />} />
    </main>
  );
}