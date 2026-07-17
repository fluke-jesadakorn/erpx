import type { ReactNode } from 'react';

export type SkeletonRounded = 'sm' | 'md' | 'lg' | 'full' | 'none';

export interface SkeletonProps {
  className?: string;
  rounded?: SkeletonRounded;
}

const ROUND: Record<SkeletonRounded, string> = {
  sm: 'rounded-sm',
  md: 'rounded-md',
  lg: 'rounded-lg',
  full: 'rounded-full',
  none: 'rounded-none',
};

export function Skeleton({ className = '', rounded = 'sm' }: SkeletonProps) {
  return <div aria-hidden className={['animate-shimmer bg-paper-3', ROUND[rounded], className].join(' ')} />;
}

export function PanelSkeleton({ rows = 3, className = '' }: { rows?: number; className?: string }) {
  return (
    <div className={['panel space-y-3 p-6', className].join(' ')}>
      <Skeleton className="h-4 w-1/3" />
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className={i % 2 === 0 ? 'h-3 w-full' : 'h-3 w-3/4'} />
      ))}
    </div>
  );
}

export function KpiSkeleton({ className = '' }: { className?: string }) {
  return (
    <div className={['panel space-y-3 p-5', className].join(' ')}>
      <Skeleton className="h-3 w-1/2" />
      <Skeleton className="h-7 w-2/3" />
      <Skeleton className="h-2 w-1/3" />
    </div>
  );
}

export function TileSkeleton({ className = '' }: { className?: string }) {
  return (
    <div className={['panel h-32 space-y-3 p-5', className].join(' ')}>
      <div className="flex items-center gap-3">
        <Skeleton className="h-9 w-9" rounded="md" />
        <Skeleton className="h-4 w-1/2" />
      </div>
      <Skeleton className="h-3 w-3/4" />
      <Skeleton className="h-2 w-full" />
    </div>
  );
}

export interface SkeletonGridProps {
  cols?: number;
  count?: number;
  renderItem?: () => ReactNode;
  className?: string;
}

export function SkeletonGrid({
  cols = 4,
  count = 8,
  renderItem,
  className = '',
}: SkeletonGridProps) {
  const grid: Record<number, string> = {
    2: 'grid-cols-2',
    3: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3',
    4: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-4',
    6: 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-6',
  };

  return (
    <div className={['grid gap-3', grid[cols] ?? grid[4], className].join(' ')}>
      {Array.from({ length: count }).map((_, i) => (
        <div key={i}>{renderItem ? renderItem() : <TileSkeleton />}</div>
      ))}
    </div>
  );
}

export function HeaderSkeleton({ className = '' }: { className?: string }) {
  return (
    <div className={['mb-8 space-y-3', className].join(' ')}>
      <Skeleton className="h-7 w-1/3" />
      <Skeleton className="h-3 w-1/2" />
    </div>
  );
}

export default Skeleton;
