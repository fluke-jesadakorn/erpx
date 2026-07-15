'use client';

import React from 'react';
import * as Tooltip from '@radix-ui/react-tooltip';

interface Props {
  content: React.ReactNode;
  children: React.ReactElement;
  side?: 'top' | 'right' | 'bottom' | 'left';
  align?: 'start' | 'center' | 'end';
  open?: boolean;
}

export const TileTooltip: React.FC<Props> = ({ content, children, side = 'top', align = 'center', open }) => {
  return (
    <Tooltip.Root delayDuration={150} open={open}>
      <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side={side}
          align={align}
          sideOffset={8}
          className="z-[60] max-w-xs rounded-xl border border-slate-700/80 bg-slate-950/95 px-3 py-2 text-sm text-slate-200 shadow-2xl shadow-black/60 backdrop-blur-sm animate-fade-in"
        >
          {content}
          <Tooltip.Arrow className="fill-slate-700/80" />
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
};

export const TileTooltipProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <Tooltip.Provider delayDuration={150} skipDelayDuration={300}>
    {children}
  </Tooltip.Provider>
);