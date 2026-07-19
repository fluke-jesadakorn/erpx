import { Tabs } from '@/components/ui';
import { T } from '@/components/i18n/TServer';
import type { SecondaryLocale } from '@/server/locale';
import { BookMarked, Gauge, History, Mail, Paperclip, type LucideIcon } from 'lucide-react';

interface TabItem {
  value: string;
  labelId: string;
  icon: LucideIcon;
  href: string;
}

interface Props {
  waybillId: string;
  active: string;
  locale?: SecondaryLocale;
}

export function WaybillTabs({ waybillId, active, locale = 'th' }: Props) {
  const items: TabItem[] = [
    { value: 'overview',    labelId: 'waybill.tabOverview',    icon: Gauge,      href: `/waybill/${waybillId}` },
    { value: 'audit',       labelId: 'waybill.tabAudit',       icon: History,    href: `/waybill/${waybillId}/audit` },
    { value: 'gl',          labelId: 'waybill.tabGl',          icon: BookMarked, href: `/waybill/${waybillId}/gl` },
    { value: 'attachments', labelId: 'waybill.tabAttachments', icon: Paperclip,  href: `/waybill/${waybillId}/attachments` },
    { value: 'chat',        labelId: 'waybill.tabChat',        icon: Mail,       href: `/waybill/${waybillId}/chat` },
  ];
  return (
    <nav aria-label="Waybill sections" className="w-full max-w-full overflow-x-auto border-b border-rule">
      <Tabs
        variant="page"
        value={active}
        className="w-max min-w-full"
        items={items.map((i) => {
          const IconCmp = i.icon;
          return {
            value: i.value,
            href: i.href,
            label: (
              <span className="inline-flex items-center gap-1.5 whitespace-nowrap">
                <IconCmp size={12} aria-hidden />
                <T id={i.labelId} locale={locale} variant="compact" />
              </span>
            ),
          };
        })}
      />
    </nav>
  );
}

export default WaybillTabs;
