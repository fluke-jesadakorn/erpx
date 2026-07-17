'use client';

import type { ComponentProps, ReactNode } from 'react';
import * as RT from '@radix-ui/react-tooltip';

export interface TooltipProviderProps {
  children: ReactNode;
  delayDuration?: number;
}

export function TooltipProvider({ children, delayDuration = 300 }: TooltipProviderProps) {
  return <RT.Provider delayDuration={delayDuration}>{children}</RT.Provider>;
}

export const Tooltip = RT.Root;
export const TooltipTrigger = RT.Trigger;

export type TooltipContentProps = ComponentProps<typeof RT.Content>;

export function TooltipContent({
  children,
  className = '',
  sideOffset = 6,
  ...props
}: TooltipContentProps) {
  return (
    <RT.Portal>
      <RT.Content
        sideOffset={sideOffset}
        className={[
          'z-[100] max-w-56 rounded-md border border-rule bg-paper px-2 py-1 text-xs text-ink-2 shadow-popover',
          className,
        ].join(' ')}
        {...props}
      >
        {children}
      </RT.Content>
    </RT.Portal>
  );
}

export default Tooltip;
