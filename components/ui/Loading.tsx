import React from 'react';

const PULSE = 'animate-pulse bg-slate-800/60';

export const Skeleton: React.FC<{
  className?: string;
  width?: string;
  height?: string;
}> = ({ className = '', width, height }) => (
  <div
    aria-hidden="true"
    className={`${PULSE} rounded ${width ?? 'w-full'} ${height ?? 'h-3'} ${className}`}
  />
);

export const PanelSkeleton: React.FC<{ rows?: number; className?: string }> = ({
  rows = 3,
  className = '',
}) => (
  <div className={`glass-panel rounded-3xl border-slate-800/60 p-6 space-y-4 ${className}`}>
    <Skeleton width="w-1/3" height="h-4" />
    {Array.from({ length: rows }).map((_, i) => (
      <Skeleton key={i} width={i % 2 === 0 ? 'w-full' : 'w-3/4'} />
    ))}
  </div>
);

export const KpiSkeleton: React.FC = () => (
  <div className="glass-panel rounded-3xl border-slate-800/60 p-5 space-y-3">
    <Skeleton width="w-1/2" height="h-3" />
    <Skeleton width="w-2/3" height="h-7" />
    <Skeleton width="w-1/3" height="h-2" />
  </div>
);

export const TileSkeleton: React.FC = () => (
  <div className="glass-panel rounded-3xl border-slate-800/60 p-5 space-y-3 h-32">
    <div className="flex items-center gap-3">
      <Skeleton width="w-9" height="h-9" className="rounded-xl" />
      <Skeleton width="w-1/2" height="h-4" />
    </div>
    <Skeleton width="w-3/4" height="h-3" />
    <Skeleton width="w-full" height="h-2" />
  </div>
);

export const SkeletonGrid: React.FC<{
  cols?: number;
  count?: number;
  renderItem?: () => React.ReactNode;
  className?: string;
}> = ({ cols = 4, count = 8, renderItem, className = '' }) => {
  const colClass: Record<number, string> = {
    2: 'grid-cols-2',
    3: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3',
    4: 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-4',
    6: 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-6',
  };
  return (
    <div className={`grid gap-3 ${colClass[cols] ?? colClass[4]} ${className}`}>
      {Array.from({ length: count }).map((_, i) => (
        <div key={i}>{renderItem ? renderItem() : <TileSkeleton />}</div>
      ))}
    </div>
  );
};

export const HeaderSkeleton: React.FC = () => (
  <div className="mb-8 space-y-3">
    <Skeleton width="w-1/3" height="h-7" />
    <Skeleton width="w-1/2" height="h-3" />
  </div>
);
