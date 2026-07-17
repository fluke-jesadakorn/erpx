'use client';

import { usePathname, useSearchParams } from 'next/navigation';
import { T } from '@/components/i18n/T';
import { Badge, Tabs } from '@/components/ui';

export type AiTabKey = 'providers' | 'models' | 'assignments' | 'my-defaults' | 'sections-health' | 'invocations';

interface TabSpec {
  key: AiTabKey;
  labelKey: string;
  count: number | null;
}

export function TabStrip({ tabs, active }: { tabs: TabSpec[]; active: AiTabKey }) {
  const params = useSearchParams();
  const pathname = usePathname();
  return (
    <Tabs
      value={active}
      variant="page"
      className="mb-4 overflow-x-auto"
      items={tabs.map((tab) => {
        const next = new URLSearchParams(params?.toString() ?? '');
        next.set('tab', tab.key);
        return {
          value: tab.key,
          href: `${pathname}?${next.toString()}`,
          label: (
            <span className="inline-flex items-center gap-2 whitespace-nowrap">
              <T id={tab.labelKey} />
              {tab.count != null && <Badge tone="neutral" size="sm">{tab.count}</Badge>}
            </span>
          ),
        };
      })}
    />
  );
}
