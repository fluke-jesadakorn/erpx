'use client';

import React from 'react';
import * as Tooltip from '@radix-ui/react-tooltip';
import { Info, type LucideIcon } from 'lucide-react';
import { T } from '@/components/i18n/T';

interface BaseProps {
  icon?: LucideIcon;
  side?: 'top' | 'right' | 'bottom' | 'left';
  className?: string;
  iconClassName?: string;
  size?: number;
}

type FromIdProps = BaseProps & {
  id: string;
  values?: Record<string, string | number>;
  content?: never;
};

type FromContentProps = BaseProps & {
  content: React.ReactNode;
  id?: never;
  values?: never;
};

type Props = FromIdProps | FromContentProps;

export const InfoHint: React.FC<Props> = ({
  icon: IconCmp = Info,
  side = 'top',
  className,
  iconClassName,
  size = 14,
  ...rest
}) => {
  const tooltipBody =
    'content' in rest && rest.content !== undefined ? (
      <>{rest.content}</>
    ) : (
      <T id={rest.id} values={rest.values} />
    );

  return (
    <Tooltip.Provider delayDuration={150} skipDelayDuration={300}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <button
            type="button"
            aria-label="More info"
            className={[
              'inline-flex items-center justify-center rounded-full text-mute hover:text-ink-2 transition-colors',
              className ?? '',
            ].join(' ')}
          >
            <IconCmp size={size} className={iconClassName} />
          </button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            side={side}
            sideOffset={8}
            className="z-popover max-w-xs rounded-md border border-rule/80 bg-paper-2/95 px-3 py-2 text-sm text-ink shadow-modal backdrop-blur-sm animate-fade-in"
          >
            {tooltipBody}
            <Tooltip.Arrow className="fill-rule/80" />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
};
