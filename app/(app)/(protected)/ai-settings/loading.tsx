import { HeaderSkeleton, PanelSkeleton, Skeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <div className="flex gap-2 border-b border-rule pb-2">
        <Skeleton className="h-8 w-24" />
        <Skeleton className="h-8 w-24" />
        <Skeleton className="h-8 w-24" />
        <Skeleton className="h-8 w-24" />
      </div>
      <PanelSkeleton rows={4} />
      <PanelSkeleton rows={4} />
    </main>
  );
}