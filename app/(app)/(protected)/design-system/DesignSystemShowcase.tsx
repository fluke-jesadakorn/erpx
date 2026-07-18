'use client';

import { useState } from 'react';
import { Bell, Info, Search, Sparkles } from 'lucide-react';
import { Alert } from '@/components/ui/Alert';
import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Kpi } from '@/components/ui/Kpi';
import { Modal } from '@/components/ui/Modal';
import { Panel } from '@/components/ui/Panel';
import { Select } from '@/components/ui/Select';
import { Skeleton } from '@/components/ui/Skeleton';
import { Status } from '@/components/ui/Status';
import { Table, type Column } from '@/components/ui/Table';
import { Tabs } from '@/components/ui/Tabs';
import { Textarea } from '@/components/ui/Textarea';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/Tooltip';
import { useToast } from '@/components/ui/Toast';

const colors = [
  ['Canvas', 'bg-canvas'],
  ['Paper', 'bg-paper'],
  ['Raised', 'bg-paper-3'],
  ['Mint', 'bg-accent'],
  ['Ice', 'bg-info'],
  ['Positive', 'bg-positive'],
  ['Caution', 'bg-caution'],
  ['Critical', 'bg-critical'],
] as const;

const rows = [
  { id: 'WB-2026-000142', state: 'In review', owner: 'Finance', amount: '฿184,200.00' },
  { id: 'WB-2026-000143', state: 'Completed', owner: 'Operations', amount: '฿32,480.00' },
  { id: 'WB-2026-000144', state: 'Rejected', owner: 'Sales', amount: '฿8,910.00' },
];

const columns: Column<(typeof rows)[number]>[] = [
  { key: 'id', header: 'Waybill', render: (row) => <span className="font-mono text-info">{row.id}</span> },
  { key: 'state', header: 'Status', render: (row) => <Status tone={row.state === 'Completed' ? 'positive' : row.state === 'Rejected' ? 'critical' : 'caution'}>{row.state}</Status> },
  { key: 'owner', header: 'Owner', render: (row) => row.owner },
  { key: 'amount', header: 'Amount', align: 'right', render: (row) => <span className="font-mono tabular-nums">{row.amount}</span> },
];

function Section({ title, note, children }: { title: string; note: string; children: React.ReactNode }) {
  return (
    <section className="space-y-3">
      <div>
        <h2 className="section-title text-ink">{title}</h2>
        <p className="mt-1 text-sm text-mute">{note}</p>
      </div>
      {children}
    </section>
  );
}

export function DesignSystemShowcase() {
  const [tab, setTab] = useState('overview');
  const [open, setOpen] = useState(false);
  const toast = useToast();

  return (
    <div className="space-y-10 pb-12">
      <Tabs
        value={tab}
        onValueChange={setTab}
        items={[
          { value: 'overview', label: 'Foundation' },
          { value: 'components', label: 'Components' },
          { value: 'data', label: 'Data density' },
        ]}
      />

      <Section title="Color foundation" note="Semantic aliases retain AA contrast across intentional dark and light themes.">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-8">
          {colors.map(([label, cls]) => (
            <Panel key={label} padding="sm" className="min-w-0">
              <span className={`block h-12 rounded-lg border border-rule ${cls}`} />
              <span className="mt-2 block truncate text-xs text-ink-2">{label}</span>
            </Panel>
          ))}
        </div>
      </Section>

      <Section title="Glass elevation" note="Use the lowest elevation that communicates hierarchy; floating is reserved for overlays and navigation.">
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          {(['default', 'elevated', 'floating', 'interactive'] as const).map((tone) => (
            <Panel key={tone} tone={tone} className="min-h-36">
              <Badge tone={tone === 'interactive' ? 'accent' : 'neutral'}>{tone}</Badge>
              <h3 className="mt-4 font-semibold capitalize text-ink">{tone} glass</h3>
              <p className="mt-1 text-sm text-ink-2">Blur, border, highlight, and shadow all come from the shared token layer.</p>
            </Panel>
          ))}
        </div>
      </Section>

      <Section title="Type and metrics" note="Geist leads Latin UI, Noto Sans Thai protects Thai shaping, and Geist Mono carries financial values.">
        <div className="grid gap-4 lg:grid-cols-[1.25fr_1fr]">
          <Panel tone="elevated">
            <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-accent">Executive briefing</p>
            <p className="mt-3 font-display text-3xl font-semibold tracking-[-0.045em] text-ink">Clarity at the point of decision.</p>
            <p className="mt-3 max-w-2xl text-sm text-ink-2">เอกสารการเงินที่ชัดเจน ปลอดภัย และพร้อมสำหรับการตัดสินใจ · Präzise Finanzabläufe für internationale Teams.</p>
            <p className="mt-5 font-mono text-2xl font-semibold tabular-nums text-positive">฿ 1,284,920.50</p>
          </Panel>
          <div className="grid grid-cols-2 gap-3">
            <Kpi label="Accessible" value="24" tone="accent" caption="Permission-aware routes" />
            <Kpi label="Pending" value="08" tone="caution" caption="Needs a decision" />
            <Kpi label="Completed" value="92%" tone="positive" caption="Current cycle" />
            <Kpi label="Exceptions" value="03" tone="critical" caption="Always labelled" />
          </div>
        </div>
      </Section>

      <Section title="Controls and feedback" note="All controls are touch-safe, keyboard visible, and stable in translated layouts.">
        <div className="grid gap-4 lg:grid-cols-2">
          <Panel className="space-y-4">
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="space-y-1.5 text-xs text-ink-2">Search<Input leftIcon={<Search size={15} />} placeholder="Search records" /></label>
              <label className="space-y-1.5 text-xs text-ink-2">Department<Select defaultValue="finance"><option value="finance">Finance</option><option value="operations">Operations</option></Select></label>
            </div>
            <label className="block space-y-1.5 text-xs text-ink-2">Decision note<Textarea placeholder="Add a concise reason…" /></label>
            <div className="flex flex-wrap gap-2">
              <Button variant="primary" leftIcon={<Sparkles size={14} />} onClick={() => setOpen(true)}>Open modal</Button>
              <Button variant="secondary" leftIcon={<Bell size={14} />} onClick={() => toast.info('The toast layer is mounted once at the application root.', 'Overlay system')}>Show toast</Button>
              <Button variant="positive">Approve</Button>
              <Button variant="critical">Reject</Button>
              <Button variant="ghost" disabled>Disabled</Button>
            </div>
          </Panel>
          <div className="space-y-3">
            <Alert tone="positive" title="Ready for approval">Status is communicated with a label and icon, never color alone.</Alert>
            <Alert tone="caution" title="Review required">Confirm the supporting document before continuing.</Alert>
            <Alert tone="critical" title="Validation failed">The error remains readable on opaque glass fallbacks.</Alert>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Badge tone="positive" dot>Completed</Badge>
          <Badge tone="caution" dot>Pending</Badge>
          <Badge tone="critical" dot>Rejected</Badge>
          <Badge tone="info" dot>Informational</Badge>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild><Button variant="secondary" size="sm" leftIcon={<Info size={13} />}>Keyboard tooltip</Button></TooltipTrigger>
              <TooltipContent>Tooltips render above drawers and below modal dialogs.</TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      </Section>

      <Section title="Productivity data" note="Tables remain compact, horizontally scrollable, and optionally sticky on narrow screens.">
        <Table columns={columns} rows={rows} rowKey={(row) => row.id} stickyHeader />
      </Section>

      <Section title="Loading language" note="Skeleton geometry mirrors final content and decorative motion respects reduced-motion preferences.">
        <Panel className="space-y-3">
          <Skeleton className="h-4 w-36" />
          <Skeleton className="h-9 w-2/3" rounded="md" />
          <Skeleton className="h-3 w-full" />
          <Skeleton className="h-3 w-4/5" />
        </Panel>
      </Section>

      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title="Review decision"
        subtitle="Focus is contained until the dialog closes."
        tone="indigo"
        footer={
          <>
            <Button variant="secondary" onClick={() => setOpen(false)}>Cancel</Button>
            <Button variant="primary" onClick={() => { setOpen(false); toast.success('Decision recorded in the showcase.'); }}>Confirm</Button>
          </>
        }
      >
        <p className="text-sm text-ink-2">The centralized modal layer handles body locking, escape, backdrop behavior, focus entry, and tab containment.</p>
      </Modal>
    </div>
  );
}
