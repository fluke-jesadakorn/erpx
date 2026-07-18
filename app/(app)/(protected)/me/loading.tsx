import { HeaderSkeleton, PanelSkeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <PanelSkeleton rows={4} />
      <div className="grid gap-6 lg:grid-cols-3">
        <PanelSkeleton rows={5} />
        <PanelSkeleton rows={5} />
        <PanelSkeleton rows={5} />
      </div>
    </main>
  );
}