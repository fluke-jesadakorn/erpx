import {
  HeaderSkeleton,
  PanelSkeleton,
  SkeletonGrid,
} from '@/components/ui/Loading';

export default function Loading() {
  return (
    <div className="animate-fade-in space-y-6">
      <HeaderSkeleton />
      <SkeletonGrid cols={2} count={2} renderItem={() => <PanelSkeleton rows={6} />} />
    </div>
  );
}
