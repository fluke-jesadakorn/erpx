import { HeaderSkeleton, PanelSkeleton } from '@/components/ui/Skeleton';

export default function Loading() {
  return (
    <main className="p-6 space-y-6">
      <HeaderSkeleton />
      <PanelSkeleton rows={4} />
      <PanelSkeleton rows={4} />
    </main>
  );
}