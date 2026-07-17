import { Tabs } from '@/components/ui';
import { Icon, type IconName } from '@/components/icons';

interface TabItem {
  value: string;
  label: string;
  icon: IconName;
  href: string;
}

interface Props {
  waybillId: string;
  active: string;
}

export function WaybillTabs({ waybillId, active }: Props) {
  const items: TabItem[] = [
    { value: 'overview',    label: 'Overview',    icon: 'gauge',    href: `/waybill/${waybillId}` },
    { value: 'audit',       label: 'Audit',       icon: 'history',  href: `/waybill/${waybillId}/audit` },
    { value: 'gl',          label: 'GL',          icon: 'wb-ledger', href: `/waybill/${waybillId}/gl` },
    { value: 'attachments', label: 'Attachments', icon: 'paperclip', href: `/waybill/${waybillId}/attachments` },
    { value: 'chat',        label: 'Chat',        icon: 'mail',     href: `/waybill/${waybillId}/chat` },
  ];
  return (
    <nav aria-label="Waybill sections" className="border-b border-rule">
      <Tabs
        variant="page"
        value={active}
        items={items.map((i) => ({
          value: i.value,
          href: i.href,
          label: (
            <span className="inline-flex items-center gap-1.5">
              <Icon name={i.icon} size={12} aria-hidden />
              {i.label}
            </span>
          ),
        }))}
      />
    </nav>
  );
}

export default WaybillTabs;