import { AlertTriangle, CheckCircle2, Eye, GitCompareArrows, LockKeyhole } from 'lucide-react';
import { Badge, type BadgeTone } from '@/components/ui/Badge';
import { Panel } from '@/components/ui/Panel';
import type { FolioBridgeReadStatus as BridgeReadStatus } from '@/platform/bridge-contract';

function reconciliationTone(state: BridgeReadStatus['reconciliation']['state']): BadgeTone {
  if (state === 'reconciled') return 'positive';
  if (state === 'drift') return 'critical';
  if (state === 'unavailable') return 'caution';
  return 'neutral';
}

function count(value: number | null): string {
  return value == null ? '—' : value.toLocaleString('en-US');
}

export function FolioBridgeReadStatus({ status }: { status: BridgeReadStatus }) {
  const { reconciliation } = status;
  const stateLabel = reconciliation.state.replaceAll('_', ' ');
  return (
    <Panel tone="elevated" padding="md" className="border-info/25">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <Eye size={16} className="text-info" />
            <h2 className="font-semibold text-ink">Folio ↔ ERPX bridge</h2>
            <Badge tone="info" size="sm">read-only</Badge>
          </div>
          <p className="mt-1 text-xs leading-5 text-mute">
            Authenticated Folio views expose bounded read evidence only. This surface does not start cutover or retire Folio writes.
          </p>
        </div>
        <Badge tone={reconciliationTone(reconciliation.state)} size="sm" dot>{stateLabel}</Badge>
      </div>
      <div className="mt-4 grid gap-2 text-xs sm:grid-cols-2 lg:grid-cols-4">
        <BridgeFact icon={<GitCompareArrows size={13} />} label="Reconciliation" value={`${count(reconciliation.counts.sourceCustomerCount)} source · ${count(reconciliation.counts.targetCustomerCount)} target`} />
        <BridgeFact icon={<CheckCircle2 size={13} />} label="Identity links" value={`${count(reconciliation.counts.targetLinkedCustomerCount)} linked · ${count(reconciliation.counts.targetUnlinkedCustomerCount)} unlinked`} />
        <BridgeFact icon={<LockKeyhole size={13} />} label="Writes / ownership" value="Folio retained" />
        <BridgeFact icon={<AlertTriangle size={13} />} label="Cutover" value="Not approved" />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-mute">
        <span>Read path: {status.readPath === 'isolated_control_read' ? 'isolated ERPX control read' : 'not configured'}</span>
        <span>Observed {new Date(reconciliation.observedAt).toLocaleString('en-GB')}</span>
      </div>
    </Panel>
  );
}

function BridgeFact({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="rounded-lg border border-rule bg-paper-3/25 px-3 py-2.5">
      <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-[0.12em] text-mute">{icon}{label}</div>
      <div className="mt-1 font-medium text-ink-2">{value}</div>
    </div>
  );
}
