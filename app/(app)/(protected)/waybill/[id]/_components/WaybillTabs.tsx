import { Tabs } from '@/components/ui';
import { BookMarked, Gauge, History, Mail, Paperclip, type LucideIcon } from 'lucide-react';

interface TabItem {
  value: string;
  label: string;
  icon: LucideIcon;
  href: string;
}

interface Props {
  waybillId: string;
  active: string;
}

export function WaybillTabs({ waybillId, active }: Props) {
  const items: TabItem[] = [
    { value: 'overview',    label: 'Overview',    icon: Gauge,     href: `/waybill/${waybillId}` },
    { value: 'audit',       label: 'Audit',       icon: History,   href: `/waybill/${waybillId}/audit` },
    { value: 'gl',          label: 'GL',          icon: BookMarked, href: `/waybill/${waybillId}/gl` },
    { value: 'attachments', label: 'Attachments', icon: Paperclip, href: `/waybill/${waybillId}/attachments` },
    { value: 'chat',        label: 'Chat',        icon: Mail,      href: `/waybill/${waybillId}/chat` },
  ];
  return (
    <nav aria-label="Waybill sections" className="border-b border-rule">
      <Tabs
        variant="page"
        value={active}
        items={items.map((i) => {
          const IconCmp = i.icon;
          return {
            value: i.value,
            href: i.href,
            label: (
              <span className="inline-flex items-center gap-1.5">
                <IconCmp size={12} aria-hidden />
                {i.label}
              </span>
            ),
          };
        })}
      />
    </nav>
  );
}

export default WaybillTabs;
