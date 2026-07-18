import { HeaderSkeleton, PanelSkeleton, Skeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <div className="flex gap-2">
        <Skeleton className="h-9 flex-1" />
        <Skeleton className="h-9 w-24" />
      </div>
      <PanelSkeleton rows={8} />
    </main>
  );
}