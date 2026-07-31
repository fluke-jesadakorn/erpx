'use client';

import { useMemo, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  ArrowUpRight,
  Building2,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  Clock3,
  LifeBuoy,
  RefreshCw,
  Search,
  ShieldCheck,
  Ticket,
  UsersRound,
  X,
} from 'lucide-react';
import { Badge } from '@/components/ui/Badge';
import { Kpi } from '@/components/ui/Kpi';
import { Panel } from '@/components/ui/Panel';
import { Status } from '@/components/ui/Status';
import type {
  PlatformCustomerDetail,
  PlatformCustomerRow,
  PlatformOverview,
  PlatformRoleAssignment,
  PlatformTicket,
} from '@/platform/types';

interface PlatformConsoleProps {
  initialOverview: PlatformOverview | null;
  schemaReady: boolean;
}

interface PlatformSupportSession {
  id: string;
  ticket_id: string;
  customer_account_id: string;
  agent_name: string;
  represented_tenant_user: string | null;
  mode: string;
  permissions: string[];
  scope: Record<string, unknown>;
  reason: string;
  requested_duration_minutes: number;
  approved_by: string | null;
  approved_at: string | null;
  requested_at: string;
  expires_at: string;
  revoked_at: string | null;
  status: string;
}

type RoleDialog =
  | { kind: 'create' }
  | { kind: 'renew'; assignment: PlatformRoleAssignment }
  | { kind: 'revoke'; assignment: PlatformRoleAssignment };

const HIGH_RISK_TIERS = new Set(['platform_root', 'administrator', 'high']);

const dateFormatter = new Intl.DateTimeFormat('en-GB', { day: '2-digit', month: 'short', year: 'numeric' });
const dateTimeFormatter = new Intl.DateTimeFormat('en-GB', { day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' });

function formatDate(value: string | null, withTime = false): string {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '—' : (withTime ? dateTimeFormatter.format(date) : dateFormatter.format(date));
}

function formatMoney(value: string): string {
  const amount = Number(value);
  return Number.isFinite(amount) ? `฿${amount.toLocaleString('en-US', { maximumFractionDigits: 0 })}` : '—';
}

function formatBytes(value: number): string {
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} GB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} TB`;
}

function toneForSubscription(value: PlatformCustomerRow['subscription_state']): 'positive' | 'caution' | 'critical' | 'info' | 'neutral' {
  if (value === 'active') return 'positive';
  if (value === 'trialing') return 'info';
  if (value === 'past_due' || value === 'grace') return 'caution';
  if (value === 'read_only' || value === 'canceled') return 'critical';
  return 'neutral';
}

function toneForHealth(value: PlatformCustomerRow['health']): 'positive' | 'caution' | 'critical' | 'neutral' {
  if (value === 'healthy') return 'positive';
  if (value === 'degraded') return 'caution';
  if (value === 'down') return 'critical';
  return 'neutral';
}

function severityTone(value: PlatformTicket['severity'] | 'none'): 'positive' | 'caution' | 'critical' | 'info' | 'neutral' {
  if (value === 'sev1') return 'critical';
  if (value === 'sev2') return 'caution';
  if (value === 'sev3') return 'info';
  if (value === 'sev4') return 'neutral';
  return 'positive';
}

function scopeLabel(role: PlatformRoleAssignment): string {
  return role.scope_label === 'Platform-wide' ? role.scope_label : role.scope_label;
}

function isHighRiskRole(role: PlatformRoleAssignment): boolean {
  return HIGH_RISK_TIERS.has(role.risk_tier);
}

function sessionTone(status: string): 'positive' | 'caution' | 'critical' | 'info' | 'neutral' {
  if (status === 'active') return 'positive';
  if (status === 'approved') return 'info';
  if (status === 'requested') return 'caution';
  if (status === 'revoked' || status === 'expired' || status === 'closed') return 'critical';
  return 'neutral';
}

export function PlatformConsole({ initialOverview, schemaReady }: PlatformConsoleProps) {
  const [overview, setOverview] = useState(initialOverview);
  const [search, setSearch] = useState('');
  const [subscription, setSubscription] = useState('all');
  const [health, setHealth] = useState('all');
  const [risk, setRisk] = useState('all');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<PlatformCustomerDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [sessions, setSessions] = useState<PlatformSupportSession[]>([]);
  const [sessionLoading, setSessionLoading] = useState(false);
  const [showTicketForm, setShowTicketForm] = useState(false);
  const [showSessionForm, setShowSessionForm] = useState(false);
  const [approvalSessionId, setApprovalSessionId] = useState<string | null>(null);
  const [roleDialog, setRoleDialog] = useState<RoleDialog | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const customers = useMemo(() => {
    const normalizedSearch = search.trim().toLowerCase();
    return (overview?.customers ?? []).filter((customer) => {
      if (normalizedSearch && !`${customer.trading_name} ${customer.legal_name ?? ''} ${customer.account_code}`.toLowerCase().includes(normalizedSearch)) return false;
      if (subscription !== 'all' && customer.subscription_state !== subscription) return false;
      if (health !== 'all' && customer.health !== health) return false;
      if (risk === 'roles' && customer.expiring_roles === 0) return false;
      if (risk === 'support' && customer.open_tickets === 0) return false;
      return true;
    });
  }, [health, overview, risk, search, subscription]);

  async function selectCustomer(customer: PlatformCustomerRow) {
    setSelectedId(customer.id);
    setDetail(null);
    setDetailLoading(true);
    setSessionLoading(true);
    setSessions([]);
    setNotice(null);
    try {
      const [detailResponse, sessionsResponse] = await Promise.all([
        fetch(`/api/platform/customers/${customer.id}`, { cache: 'no-store' }),
        fetch(`/api/platform/sessions?customerId=${encodeURIComponent(customer.id)}`, { cache: 'no-store' }),
      ]);
      const [payload, sessionsPayload] = await Promise.all([
        detailResponse.json() as Promise<{ detail?: PlatformCustomerDetail; error?: string }>,
        sessionsResponse.json() as Promise<{ sessions?: PlatformSupportSession[]; error?: string }>,
      ]);
      if (!detailResponse.ok || !payload.detail) throw new Error(payload.error ?? 'Unable to load customer detail');
      if (!sessionsResponse.ok || !sessionsPayload.sessions) throw new Error(sessionsPayload.error ?? 'Unable to load support approvals');
      setDetail(payload.detail);
      setSessions(sessionsPayload.sessions);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to load customer detail');
    } finally {
      setDetailLoading(false);
      setSessionLoading(false);
    }
  }

  async function reloadRoles() {
    const response = await fetch('/api/platform/roles', { cache: 'no-store' });
    const payload = await response.json() as { assignments?: PlatformRoleAssignment[]; error?: string };
    if (!response.ok || !payload.assignments) throw new Error(payload.error ?? 'Unable to load role assignments');
    setOverview((current) => current ? { ...current, roles: payload.assignments ?? [] } : current);
  }

  async function refresh() {
    setBusy(true);
    try {
      const params = new URLSearchParams();
      if (search.trim()) params.set('q', search.trim());
      if (subscription !== 'all') params.set('subscription', subscription);
      if (health !== 'all') params.set('health', health);
      if (risk !== 'all') params.set('risk', risk);
      const response = await fetch(`/api/platform/customers?${params.toString()}`, { cache: 'no-store' });
      const payload = await response.json() as { customers?: PlatformCustomerRow[]; error?: string };
      if (!response.ok || !payload.customers) throw new Error(payload.error ?? 'Unable to refresh customers');
      setOverview((current) => current ? { ...current, customers: payload.customers ?? [] } : current);
      await reloadRoles();
      setNotice('Customer list refreshed.');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to refresh customers');
    } finally {
      setBusy(false);
    }
  }

  async function createTicket(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!detail) return;
    setBusy(true);
    const form = new FormData(event.currentTarget);
    try {
      const response = await fetch('/api/platform/tickets', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          customerId: detail.customer.id,
          moduleKey: form.get('moduleKey'),
          severity: form.get('severity'),
          category: form.get('category'),
          affectedUsers: Number(form.get('affectedUsers') ?? 0),
          description: form.get('description'),
          diagnosticsConsent: form.get('diagnosticsConsent') === 'on',
        }),
      });
      const payload = await response.json() as { ticket?: PlatformTicket; error?: string };
      if (!response.ok || !payload.ticket) throw new Error(payload.error ?? 'Unable to create ticket');
      setDetail((current) => current ? { ...current, tickets: [payload.ticket as PlatformTicket, ...current.tickets] } : current);
      setShowTicketForm(false);
      setNotice(`${payload.ticket.ticket_number} created. Support access still requires customer approval.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to create ticket');
    } finally {
      setBusy(false);
    }
  }

  async function requestSession(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!detail) return;
    setBusy(true);
    const form = new FormData(event.currentTarget);
    const ticketId = String(form.get('ticketId') ?? '');
    try {
      const response = await fetch('/api/platform/sessions', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          customerId: detail.customer.id,
          ticketId,
          mode: form.get('mode'),
          durationMinutes: Number(form.get('durationMinutes') ?? 60),
          representedTenantUser: form.get('representedTenantUser') || null,
          reason: form.get('reason'),
          scope: { ticket: ticketId, modules: [form.get('moduleKey')].filter(Boolean) },
        }),
      });
      const payload = await response.json() as { session?: { expires_at: string }; error?: string };
      if (!response.ok || !payload.session) throw new Error(payload.error ?? 'Unable to request support session');
      setShowSessionForm(false);
      await selectCustomer(detail.customer);
      setNotice(`Access request created and expires ${formatDate(payload.session.expires_at, true)} after approval.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to request support session');
    } finally {
      setBusy(false);
    }
  }

  async function approveSession(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!detail || !approvalSessionId) return;
    setBusy(true);
    const form = new FormData(event.currentTarget);
    try {
      const response = await fetch(`/api/platform/sessions/${approvalSessionId}/approve`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          customerId: detail.customer.id,
          approver: form.get('approver'),
          approvalToken: form.get('approvalToken'),
        }),
      });
      const payload = await response.json() as { session?: PlatformSupportSession; error?: string };
      if (!response.ok || !payload.session) throw new Error(payload.error ?? 'Unable to approve support access');
      setApprovalSessionId(null);
      await selectCustomer(detail.customer);
      setNotice('Customer approval recorded. The support capability remains scoped and auditable.');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to approve support access');
    } finally {
      setBusy(false);
    }
  }

  async function revokeSession(sessionId: string) {
    if (!detail || !window.confirm('Revoke this support capability now?')) return;
    setBusy(true);
    try {
      const response = await fetch(`/api/platform/sessions/${sessionId}`, { method: 'DELETE' });
      const payload = await response.json() as { error?: string };
      if (!response.ok) throw new Error(payload.error ?? 'Unable to revoke support access');
      await selectCustomer(detail.customer);
      setNotice('Support capability revoked and the revocation was recorded.');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to revoke support access');
    } finally {
      setBusy(false);
    }
  }

  async function createRole(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    const form = new FormData(event.currentTarget);
    const riskTier = String(form.get('riskTier') ?? 'standard');
    const stepUpEvidence = String(form.get('stepUpEvidence') ?? '').trim();
    if (HIGH_RISK_TIERS.has(riskTier) && !stepUpEvidence) {
      setNotice('High-risk role creation requires a signed step-up MFA evidence token.');
      setBusy(false);
      return;
    }
    try {
      const headers: Record<string, string> = { 'content-type': 'application/json' };
      if (stepUpEvidence) headers['x-erpx-step-up-evidence'] = stepUpEvidence;
      const response = await fetch('/api/platform/roles', {
        method: 'POST',
        headers,
        body: JSON.stringify({
          principalRef: form.get('principalRef'),
          principalType: form.get('principalType'),
          roleKey: form.get('roleKey'),
          permissions: String(form.get('permissions') ?? '').split(',').map((item) => item.trim()).filter(Boolean),
          functionalDomain: form.get('functionalDomain'),
          authorityLevel: Number(form.get('authorityLevel') ?? 99),
          holdingId: form.get('holdingId') || null,
          companyId: form.get('companyId') || null,
          branchId: form.get('branchId') || null,
          durationMinutes: Number(form.get('durationMinutes') ?? 60),
          timezone: form.get('timezone'),
          riskTier,
          grantReason: form.get('grantReason'),
          ticketReference: form.get('ticketReference') || null,
          contractReference: form.get('contractReference') || null,
        }),
      });
      const payload = await response.json() as { assignment?: PlatformRoleAssignment; error?: string };
      if (!response.ok || !payload.assignment) throw new Error(payload.error ?? 'Unable to create role assignment');
      await reloadRoles();
      setRoleDialog(null);
      setNotice('Role assignment created with an attributable grant record.');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to create role assignment');
    } finally {
      setBusy(false);
    }
  }

  async function renewRole(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!roleDialog || roleDialog.kind !== 'renew') return;
    setBusy(true);
    const form = new FormData(event.currentTarget);
    const stepUpEvidence = String(form.get('stepUpEvidence') ?? '').trim();
    if (isHighRiskRole(roleDialog.assignment) && !stepUpEvidence) {
      setNotice('High-risk role renewal requires a signed step-up MFA evidence token.');
      setBusy(false);
      return;
    }
    try {
      const headers: Record<string, string> = { 'content-type': 'application/json' };
      if (stepUpEvidence) headers['x-erpx-step-up-evidence'] = stepUpEvidence;
      const response = await fetch(`/api/platform/roles/${roleDialog.assignment.id}/renew`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ durationMinutes: Number(form.get('durationMinutes') ?? 60) }),
      });
      const payload = await response.json() as { assignment?: PlatformRoleAssignment; error?: string };
      if (!response.ok || !payload.assignment) throw new Error(payload.error ?? 'Unable to renew role assignment');
      await reloadRoles();
      setRoleDialog(null);
      setNotice(`Role renewed as append-only version ${payload.assignment.assignment_version}; the prior version was revoked.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to renew role assignment');
    } finally {
      setBusy(false);
    }
  }

  async function revokeRole(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!roleDialog || roleDialog.kind !== 'revoke') return;
    setBusy(true);
    const form = new FormData(event.currentTarget);
    const stepUpEvidence = String(form.get('stepUpEvidence') ?? '').trim();
    if (isHighRiskRole(roleDialog.assignment) && !stepUpEvidence) {
      setNotice('High-risk role revocation requires a signed step-up MFA evidence token.');
      setBusy(false);
      return;
    }
    try {
      const headers: Record<string, string> = {};
      if (stepUpEvidence) headers['x-erpx-step-up-evidence'] = stepUpEvidence;
      const response = await fetch(`/api/platform/roles/${roleDialog.assignment.id}`, { method: 'DELETE', headers });
      const payload = await response.json() as { error?: string };
      if (!response.ok) throw new Error(payload.error ?? 'Unable to revoke role assignment');
      await reloadRoles();
      setRoleDialog(null);
      setNotice('Role assignment revoked append-only; its historical record remains available.');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Unable to revoke role assignment');
    } finally {
      setBusy(false);
    }
  }

  if (!schemaReady || !overview) {
    return (
      <Panel tone="elevated" padding="lg" className="border-caution/35">
        <div className="flex items-start gap-4">
          <AlertTriangle className="mt-0.5 shrink-0 text-caution" size={20} />
          <div>
            <h2 className="text-lg font-semibold text-ink">Control plane is not initialized</h2>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-ink-2">
              Apply the ERPX control-plane migration before opening the console. It creates the separate platform metadata, support, entitlement, audit, and expiring-assignment tables without changing tenant business records.
            </p>
            <code className="mt-4 inline-flex rounded-lg border border-rule bg-paper-3 px-3 py-2 text-xs text-ink-2">bun run db:setup</code>
          </div>
        </div>
      </Panel>
    );
  }

  const selectedCustomer = selectedId ? overview.customers.find((customer) => customer.id === selectedId) ?? detail?.customer : null;
  const activeDetail = detail?.customer.id === selectedId ? detail : null;

  return (
    <div className="space-y-5">
      {notice && (
        <div className="flex items-center justify-between gap-3 rounded-xl border border-info/30 bg-info-soft/20 px-4 py-3 text-sm text-info">
          <span>{notice}</span>
          <button type="button" onClick={() => setNotice(null)} aria-label="Dismiss notification"><X size={15} /></button>
        </div>
      )}

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Customers" value={overview.metrics.total_customers} tone="info" size="sm" caption="all accounts" />
        <Kpi label="Active" value={overview.metrics.active_customers} tone="positive" size="sm" caption="operational" />
        <Kpi label="Payment risk" value={overview.metrics.payment_risk} tone="caution" size="sm" caption="past due / grace" />
        <Kpi label="Open support" value={overview.metrics.open_tickets} tone="accent" size="sm" caption="tickets" />
        <Kpi label="Role expiry" value={overview.metrics.roles_expiring_30d} tone="critical" size="sm" caption="next 30 days" />
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1.35fr)_minmax(360px,0.65fr)]">
        <Panel padding="none" className="min-w-0 overflow-hidden">
          <div className="flex flex-wrap items-center gap-2 border-b border-rule px-4 py-3">
            <div className="glass-input flex min-w-[220px] flex-1 items-center gap-2 px-3 py-2">
              <Search size={15} className="text-mute" />
              <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search account, trading name, code" className="w-full bg-transparent text-sm outline-none placeholder:text-mute" />
            </div>
            <select value={subscription} onChange={(event) => setSubscription(event.target.value)} className="glass-input px-3 py-2 text-sm">
              <option value="all">All subscription states</option>
              <option value="active">Active</option>
              <option value="trialing">Trialing</option>
              <option value="past_due">Past due</option>
              <option value="grace">Grace</option>
              <option value="read_only">Read-only</option>
            </select>
            <select value={health} onChange={(event) => setHealth(event.target.value)} className="glass-input px-3 py-2 text-sm">
              <option value="all">All health</option>
              <option value="healthy">Healthy</option>
              <option value="degraded">Degraded</option>
              <option value="down">Down</option>
            </select>
            <select value={risk} onChange={(event) => setRisk(event.target.value)} className="glass-input px-3 py-2 text-sm">
              <option value="all">All risk</option>
              <option value="roles">Role expiry</option>
              <option value="support">Support queue</option>
            </select>
            <button type="button" onClick={refresh} disabled={busy} className="glass-chip inline-flex h-9 items-center gap-2 px-3 text-sm text-ink-2 hover:text-ink disabled:opacity-50">
              <RefreshCw size={14} className={busy ? 'animate-spin' : ''} /> Refresh
            </button>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full min-w-[760px] text-sm">
              <thead className="border-b border-rule bg-paper-3/30 text-left text-[10px] uppercase tracking-[0.14em] text-mute">
                <tr>
                  <th className="px-4 py-3 font-medium">Account</th>
                  <th className="px-3 py-3 font-medium">Commercial</th>
                  <th className="px-3 py-3 font-medium">Structure</th>
                  <th className="px-3 py-3 font-medium">Health</th>
                  <th className="px-3 py-3 font-medium">Access risk</th>
                  <th className="px-4 py-3" />
                </tr>
              </thead>
              <tbody>
                {customers.length === 0 ? (
                  <tr><td colSpan={6} className="px-4 py-12 text-center text-sm text-mute">No customer accounts match these filters.</td></tr>
                ) : customers.map((customer) => (
                  <tr key={customer.id} className={`cursor-pointer border-b border-rule/70 transition-colors hover:bg-paper-3/40 ${selectedId === customer.id ? 'bg-info-soft/15' : ''}`} onClick={() => void selectCustomer(customer)}>
                    <td className="px-4 py-3">
                      <div className="flex items-center gap-3">
                        <div className="grid h-8 w-8 place-items-center rounded-lg border border-info/30 bg-info-soft/25 text-info"><Building2 size={15} /></div>
                        <div className="min-w-0"><div className="truncate font-medium text-ink">{customer.trading_name}</div><div className="font-mono text-[11px] text-mute">{customer.account_code} · {customer.country}</div></div>
                      </div>
                    </td>
                    <td className="px-3 py-3"><div><Badge tone={toneForSubscription(customer.subscription_state)} size="sm" dot>{customer.subscription_state.replace('_', ' ')}</Badge></div><div className="mt-1 text-[11px] text-mute">renewal {formatDate(customer.renewal_at)}</div></td>
                    <td className="px-3 py-3"><div className="font-mono text-xs text-ink-2">{customer.companies_count} co · {customer.branches_count} br</div><div className="mt-1 text-[11px] text-mute">{customer.active_users} active users · {customer.module_count} modules</div></td>
                    <td className="px-3 py-3"><Status tone={toneForHealth(customer.health)} size="sm">{customer.health}</Status><div className="mt-1 text-[11px] text-mute">lag {Math.round(customer.projection_lag_seconds)}s</div></td>
                    <td className="px-3 py-3"><div className="flex flex-wrap gap-1.5">{customer.expiring_roles > 0 && <Badge tone="caution" size="sm"><Clock3 size={11} /> {customer.expiring_roles}</Badge>}{customer.open_tickets > 0 && <Badge tone={severityTone(customer.support_severity)} size="sm"><Ticket size={11} /> {customer.open_tickets}</Badge>}{customer.expiring_roles === 0 && customer.open_tickets === 0 && <span className="text-xs text-mute">clear</span>}</div></td>
                    <td className="px-4 py-3 text-right text-mute"><ChevronRight size={16} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Panel>

        <Panel padding="none" className="overflow-hidden">
          {!selectedCustomer ? (
            <div className="grid min-h-[420px] place-items-center p-8 text-center"><div><CircleDot size={26} className="mx-auto text-info/70" /><h2 className="mt-4 text-base font-semibold text-ink">Select a customer account</h2><p className="mt-2 max-w-xs text-sm leading-6 text-mute">Opening a customer records an attributable global-read audit event. Secrets and credentials are never returned here.</p></div></div>
          ) : detailLoading ? (
            <div className="grid min-h-[420px] place-items-center text-sm text-mute"><RefreshCw size={17} className="mr-2 animate-spin" /> Loading audited customer view…</div>
          ) : activeDetail ? (
            <CustomerDetail
              detail={activeDetail}
              sessions={sessions}
              sessionLoading={sessionLoading}
              onClose={() => { setSelectedId(null); setDetail(null); setSessions([]); }}
              onTicket={() => setShowTicketForm(true)}
              onSession={() => setShowSessionForm(true)}
              onApprove={(sessionId) => setApprovalSessionId(sessionId)}
              onRevoke={(sessionId) => void revokeSession(sessionId)}
            />
          ) : (
            <div className="p-6 text-sm text-mute">Unable to load this account. {notice ?? ''}</div>
          )}
        </Panel>
      </div>

      {activeDetail && showTicketForm && <TicketForm busy={busy} onSubmit={createTicket} onClose={() => setShowTicketForm(false)} />}
      {activeDetail && showSessionForm && <SessionForm detail={activeDetail} busy={busy} onSubmit={requestSession} onClose={() => setShowSessionForm(false)} />}
      {approvalSessionId && sessions.find((session) => session.id === approvalSessionId) && <SessionApprovalForm session={sessions.find((session) => session.id === approvalSessionId) as PlatformSupportSession} busy={busy} onSubmit={approveSession} onClose={() => setApprovalSessionId(null)} />}
      <RoleManagementPanel assignments={overview.roles} busy={busy} onCreate={() => setRoleDialog({ kind: 'create' })} onRenew={(assignment) => setRoleDialog({ kind: 'renew', assignment })} onRevoke={(assignment) => setRoleDialog({ kind: 'revoke', assignment })} />
      {roleDialog?.kind === 'create' && <RoleCreateForm busy={busy} onSubmit={createRole} onClose={() => setRoleDialog(null)} />}
      {roleDialog?.kind === 'renew' && <RoleRenewForm assignment={roleDialog.assignment} busy={busy} onSubmit={renewRole} onClose={() => setRoleDialog(null)} />}
      {roleDialog?.kind === 'revoke' && <RoleRevokeForm assignment={roleDialog.assignment} busy={busy} onSubmit={revokeRole} onClose={() => setRoleDialog(null)} />}
    </div>
  );
}

function CustomerDetail({ detail, sessions, sessionLoading, onClose, onTicket, onSession, onApprove, onRevoke }: { detail: PlatformCustomerDetail; sessions: PlatformSupportSession[]; sessionLoading: boolean; onClose: () => void; onTicket: () => void; onSession: () => void; onApprove: (sessionId: string) => void; onRevoke: (sessionId: string) => void }) {
  const { customer } = detail;
  return (
    <div className="flex h-full min-h-[420px] flex-col">
      <div className="border-b border-rule px-5 py-4">
        <div className="flex items-start justify-between gap-3"><div><div className="flex items-center gap-2"><Badge tone="info" size="sm">{customer.account_code}</Badge><Status tone={toneForHealth(customer.health)} size="sm">{customer.health}</Status></div><h2 className="mt-2 text-xl font-semibold tracking-[-0.03em] text-ink">{customer.trading_name}</h2><p className="mt-1 text-xs text-mute">{customer.legal_name ?? 'Legal name not recorded'} · {customer.industry ?? 'Industry not recorded'}</p></div><button type="button" onClick={onClose} className="rounded-lg p-1.5 text-mute hover:bg-paper-3 hover:text-ink" aria-label="Close customer detail"><X size={16} /></button></div>
        <div className="mt-4 flex flex-wrap gap-2"><button type="button" onClick={onTicket} className="inline-flex items-center gap-2 rounded-lg bg-accent px-3 py-2 text-xs font-medium text-action-ink hover:bg-accent-strong"><Ticket size={14} /> New support ticket</button><button type="button" onClick={onSession} className="inline-flex items-center gap-2 rounded-lg border border-caution/40 bg-caution-soft/20 px-3 py-2 text-xs font-medium text-caution hover:bg-caution-soft/35"><ShieldCheck size={14} /> Request scoped access</button></div>
      </div>
      <div className="flex-1 space-y-5 overflow-y-auto p-5">
        <div className="grid grid-cols-2 gap-2"><MiniMetric icon={<UsersRound size={14} />} label="Users" value={customer.active_users} /><MiniMetric icon={<Building2 size={14} />} label="Companies" value={customer.companies_count} /><MiniMetric icon={<Activity size={14} />} label="Storage" value={formatBytes(customer.storage_bytes)} /><MiniMetric icon={<ArrowUpRight size={14} />} label="LTV" value={formatMoney(customer.lifetime_value)} /></div>
        <section><SectionHeading icon={<Building2 size={14} />} title="Holding structure" /><div className="mt-2 space-y-2">{detail.holdings.length === 0 ? <p className="text-xs text-mute">No holding structure has been provisioned yet.</p> : detail.holdings.map((holding) => <div key={holding.id} className="surface-sunken p-3"><div className="flex items-center justify-between text-sm font-medium text-ink"><span>{holding.name}</span><span className="text-[11px] text-mute">{holding.companies.length} companies</span></div>{holding.companies.map((company) => <div key={company.id} className="mt-2 border-l border-info/40 pl-3 text-xs text-ink-2"><div>{company.legal_name}</div><div className="mt-1 text-mute">{company.branches.length} branches · {company.tax_id ?? 'tax ID not recorded'}</div></div>)}</div>)}</div></section>
        <section><SectionHeading icon={<CheckCircle2 size={14} />} title="Entitlements" /><div className="mt-2 flex flex-wrap gap-1.5">{detail.entitlements.length === 0 ? <span className="text-xs text-mute">No module allocations yet.</span> : detail.entitlements.map((item) => <Badge key={item.module_key} tone={item.status === 'active' ? 'positive' : 'neutral'} size="sm">{item.module_key} · {item.seats} seats</Badge>)}</div></section>
        <section><SectionHeading icon={<LifeBuoy size={14} />} title="Support and access" /><div className="mt-2 space-y-2">{detail.tickets.slice(0, 4).map((ticket) => <div key={ticket.id} className="flex items-start justify-between gap-3 rounded-lg border border-rule bg-paper-3/30 px-3 py-2"><div className="min-w-0"><div className="font-mono text-xs text-info">{ticket.ticket_number}</div><div className="mt-1 truncate text-xs text-ink-2">{ticket.description}</div></div><Badge tone={severityTone(ticket.severity)} size="sm">{ticket.status.replaceAll('_', ' ')}</Badge></div>)}{detail.tickets.length === 0 && <p className="text-xs text-mute">No support tickets for this account.</p>}</div></section>
        <section><SectionHeading icon={<ShieldCheck size={14} />} title="Access approvals" /><div className="mt-2 space-y-2">{sessionLoading ? <div className="flex items-center gap-2 text-xs text-mute"><RefreshCw size={13} className="animate-spin" /> Loading scoped capabilities…</div> : sessions.length === 0 ? <p className="text-xs text-mute">No support capability requests for this account.</p> : sessions.map((session) => <div key={session.id} className="rounded-lg border border-rule bg-paper-3/30 px-3 py-2"><div className="flex items-start justify-between gap-3"><div><div className="flex flex-wrap items-center gap-2"><span className="font-mono text-[11px] text-info">{session.id.slice(0, 8)}</span><Badge tone={sessionTone(session.status)} size="sm">{session.status}</Badge><Badge tone="neutral" size="sm">{session.mode.replace('_', ' ')}</Badge></div><div className="mt-1 text-xs text-ink-2">{session.reason}</div><div className="mt-1 text-[11px] text-mute">expires {formatDate(session.expires_at, true)} · requested by {session.agent_name}</div></div><div className="flex shrink-0 gap-1.5">{session.status === 'requested' && <button type="button" onClick={() => onApprove(session.id)} className="rounded-md border border-positive/35 bg-positive-soft/20 px-2 py-1 text-[11px] font-medium text-positive hover:bg-positive-soft/35">Approve</button>}{['requested', 'approved', 'active'].includes(session.status) && <button type="button" onClick={() => onRevoke(session.id)} className="rounded-md border border-critical/35 bg-critical-soft/20 px-2 py-1 text-[11px] font-medium text-critical hover:bg-critical-soft/35">Revoke</button>}</div></div></div>)}</div></section>
        <section><SectionHeading icon={<Clock3 size={14} />} title="Expiring role assignments" /><div className="mt-2 space-y-2">{detail.roles.filter((role) => role.valid_until && new Date(role.valid_until).getTime() <= Date.now() + 30 * 86400000).slice(0, 5).map((role) => <div key={role.id} className="flex items-center justify-between gap-3 border-b border-rule/70 pb-2 text-xs"><div><div className="font-medium text-ink">{role.role_key} · {role.principal_ref}</div><div className="mt-1 text-mute">{scopeLabel(role)} · {role.risk_tier}</div></div><span className="font-mono text-caution">{formatDate(role.valid_until)}</span></div>)}{detail.roles.filter((role) => new Date(role.valid_until).getTime() <= Date.now() + 30 * 86400000).length === 0 && <p className="text-xs text-mute">No assignments expire in the next 30 days.</p>}</div></section>
        <div className="flex items-center gap-2 text-[11px] text-mute"><ShieldCheck size={13} className="text-info" /> Global reads are logged with purpose, actor, timestamp, and correlation ID.</div>
      </div>
    </div>
  );
}

function MiniMetric({ icon, label, value }: { icon: React.ReactNode; label: string; value: React.ReactNode }) { return <div className="rounded-lg border border-rule bg-paper-3/25 p-3"><div className="flex items-center gap-1.5 text-[10px] uppercase tracking-[0.12em] text-mute">{icon}{label}</div><div className="mt-1 font-mono text-sm text-ink">{value}</div></div>; }
function SectionHeading({ icon, title }: { icon: React.ReactNode; title: string }) { return <h3 className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.13em] text-ink-2">{icon}{title}</h3>; }

function RoleManagementPanel({ assignments, busy, onCreate, onRenew, onRevoke }: { assignments: PlatformRoleAssignment[]; busy: boolean; onCreate: () => void; onRenew: (assignment: PlatformRoleAssignment) => void; onRevoke: (assignment: PlatformRoleAssignment) => void }) {
  return <Panel padding="none" className="overflow-hidden"><div className="flex flex-wrap items-center justify-between gap-3 border-b border-rule px-4 py-3"><div><h2 className="text-sm font-semibold text-ink">Role management</h2><p className="mt-1 text-xs text-mute">Versioned assignments remain auditable after renewal or revocation. High-risk changes require fresh MFA evidence.</p></div><button type="button" onClick={onCreate} disabled={busy} className="inline-flex items-center gap-2 rounded-lg bg-accent px-3 py-2 text-xs font-medium text-action-ink hover:bg-accent-strong disabled:opacity-50"><ShieldCheck size={14} /> Create assignment</button></div><div className="overflow-x-auto"><table className="w-full min-w-[900px] text-sm"><caption className="sr-only">Platform role assignments</caption><thead className="border-b border-rule bg-paper-3/30 text-left text-[10px] uppercase tracking-[0.14em] text-mute"><tr><th className="px-4 py-3 font-medium">Principal / role</th><th className="px-3 py-3 font-medium">Scope</th><th className="px-3 py-3 font-medium">Risk</th><th className="px-3 py-3 font-medium">Version / window</th><th className="px-4 py-3 text-right font-medium">Lifecycle</th></tr></thead><tbody>{assignments.length === 0 ? <tr><td colSpan={5} className="px-4 py-10 text-center text-xs text-mute">No role assignments have been recorded.</td></tr> : assignments.map((assignment) => <tr key={assignment.id} className="border-b border-rule/70"><td className="px-4 py-3"><div className="font-medium text-ink">{assignment.role_key}</div><div className="mt-1 font-mono text-[11px] text-mute">{assignment.principal_ref} · {assignment.principal_type}</div></td><td className="px-3 py-3 text-xs text-ink-2">{scopeLabel(assignment)}<div className="mt-1 text-[11px] text-mute">{assignment.functional_domain}</div></td><td className="px-3 py-3"><Badge tone={isHighRiskRole(assignment) ? 'critical' : 'neutral'} size="sm">{assignment.risk_tier}</Badge><div className="mt-1 text-[11px] text-mute">{assignment.effective ? 'effective' : 'inactive'}</div></td><td className="px-3 py-3 text-xs text-ink-2"><div className="font-mono">v{assignment.assignment_version}{assignment.renewed_from_assignment_id ? ` · renewed from ${assignment.renewed_from_assignment_id.slice(0, 8)}` : ''}</div><div className="mt-1 text-[11px] text-mute">{formatDate(assignment.valid_from, true)} → {formatDate(assignment.valid_until, true)}</div></td><td className="px-4 py-3"><div className="flex justify-end gap-1.5"><button type="button" onClick={() => onRenew(assignment)} disabled={busy || Boolean(assignment.renewed_from_assignment_id && !assignment.effective)} className="rounded-md border border-info/35 bg-info-soft/15 px-2 py-1 text-[11px] font-medium text-info hover:bg-info-soft/30 disabled:opacity-40">Renew</button><button type="button" onClick={() => onRevoke(assignment)} disabled={busy || !assignment.effective} className="rounded-md border border-critical/35 bg-critical-soft/15 px-2 py-1 text-[11px] font-medium text-critical hover:bg-critical-soft/30 disabled:opacity-40">Revoke</button></div></td></tr>)}</tbody></table></div></Panel>;
}

function TicketForm({ busy, onSubmit, onClose }: { busy: boolean; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; onClose: () => void }) {
  return <Overlay title="Create support ticket" onClose={onClose}><form onSubmit={onSubmit} className="space-y-3"><div className="grid grid-cols-2 gap-3"><Field label="Module" name="moduleKey" placeholder="finance, hr, inventory" /><Field label="Affected users" name="affectedUsers" type="number" defaultValue="0" /><label className="text-xs text-ink-2">Severity<select name="severity" defaultValue="sev3" className="field"><option value="sev1">SEV1 · critical</option><option value="sev2">SEV2 · high</option><option value="sev3">SEV3 · normal</option><option value="sev4">SEV4 · question</option></select></label><label className="text-xs text-ink-2">Category<select name="category" defaultValue="incident" className="field"><option value="incident">Incident</option><option value="question">Question</option><option value="billing">Billing</option><option value="data">Data</option></select></label></div><label className="block text-xs text-ink-2">Description<textarea name="description" minLength={10} required rows={5} className="field" placeholder="What happened, when, and which tenant scope is affected?" /></label><label className="flex items-center gap-2 text-xs text-ink-2"><input name="diagnosticsConsent" type="checkbox" /> Customer diagnostics consent recorded</label><FormButtons busy={busy} onClose={onClose} submit="Create ticket" /></form></Overlay>;
}

function SessionForm({ detail, busy, onSubmit, onClose }: { detail: PlatformCustomerDetail; busy: boolean; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; onClose: () => void }) {
  return <Overlay title="Request customer-approved access" onClose={onClose}><form onSubmit={onSubmit} className="space-y-3"><label className="block text-xs text-ink-2">Ticket<select name="ticketId" required className="field">{detail.tickets.filter((ticket) => ticket.status !== 'closed').map((ticket) => <option key={ticket.id} value={ticket.id}>{ticket.ticket_number} · {ticket.description.slice(0, 54)}</option>)}</select></label><div className="grid grid-cols-2 gap-3"><label className="text-xs text-ink-2">Mode<select name="mode" defaultValue="diagnostic_read" className="field"><option value="diagnostic_read">Diagnostic / read · max 8h</option><option value="tenant_mutation">Tenant mutation · max 2h</option><option value="impersonation">Impersonation · max 2h</option></select></label><Field label="Duration (minutes)" name="durationMinutes" type="number" defaultValue="60" min="1" max="480" /><Field label="Module scope" name="moduleKey" placeholder="finance" /><Field label="Represented tenant user" name="representedTenantUser" placeholder="required for impersonation" /></div><label className="block text-xs text-ink-2">Reason<textarea name="reason" minLength={10} required rows={4} className="field" placeholder="Explain the customer-approved purpose and exact scope." /></label><div className="rounded-lg border border-caution/30 bg-caution-soft/15 p-3 text-xs leading-5 text-caution">This creates a requested capability only. The customer Holding Owner or Security Head must approve it before any tenant mutation or impersonation can start.</div><FormButtons busy={busy} onClose={onClose} submit="Request approval" /></form></Overlay>;
}

function SessionApprovalForm({ session, busy, onSubmit, onClose }: { session: PlatformSupportSession; busy: boolean; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; onClose: () => void }) {
  return <Overlay title="Approve scoped support access" onClose={onClose}><form onSubmit={onSubmit} className="space-y-3"><div className="rounded-lg border border-info/30 bg-info-soft/15 p-3 text-xs leading-5 text-ink-2">Approval is accepted only with the customer-signed token for session <span className="font-mono">{session.id}</span>. The token is verified server-side and is never stored in the browser UI.</div><label className="block text-xs text-ink-2">Customer approver<input name="approver" required maxLength={200} className="field" placeholder="Holding Owner or Security Head" /></label><label className="block text-xs text-ink-2">Customer approval token<input name="approvalToken" required maxLength={256} type="password" className="field" autoComplete="off" /></label><FormButtons busy={busy} onClose={onClose} submit="Record approval" /></form></Overlay>;
}

function RoleCreateForm({ busy, onSubmit, onClose }: { busy: boolean; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; onClose: () => void }) {
  return <Overlay title="Create role assignment" onClose={onClose}><form onSubmit={onSubmit} className="space-y-3"><div className="grid grid-cols-2 gap-3"><Field label="Principal reference" name="principalRef" placeholder="employee or service identity" /><label className="text-xs text-ink-2">Principal type<select name="principalType" defaultValue="tenant_user" className="field"><option value="tenant_user">Tenant user</option><option value="platform_user">Platform user</option><option value="service">Service</option></select></label><Field label="Role key" name="roleKey" placeholder="finance.approver" /><Field label="Functional domain" name="functionalDomain" defaultValue="organization" /><label className="text-xs text-ink-2">Risk tier<select name="riskTier" defaultValue="standard" className="field"><option value="standard">Standard</option><option value="high">High</option><option value="administrator">Administrator</option><option value="platform_root">Platform root</option><option value="contractor">Contractor</option><option value="support">Support</option><option value="emergency">Emergency</option></select></label><Field label="Authority level" name="authorityLevel" type="number" defaultValue="99" min="1" max="99" /><Field label="Duration (minutes)" name="durationMinutes" type="number" defaultValue="60" min="1" /><Field label="Timezone" name="timezone" defaultValue="UTC" /></div><Field label="Permissions (comma separated)" name="permissions" placeholder="finance:journal:approve::allow" /><div className="grid grid-cols-3 gap-3"><Field label="Holding ID" name="holdingId" placeholder="optional UUID" /><Field label="Company ID" name="companyId" placeholder="optional UUID" /><Field label="Branch ID" name="branchId" placeholder="optional UUID" /></div><div className="grid grid-cols-2 gap-3"><Field label="Ticket reference" name="ticketReference" /><Field label="Contract reference" name="contractReference" /></div><label className="block text-xs text-ink-2">Grant reason<textarea name="grantReason" minLength={10} required rows={3} className="field" placeholder="State the exact business purpose and boundary." /></label><label className="block text-xs text-ink-2">Signed step-up MFA evidence <span className="text-mute">(required for high-risk tiers)</span><textarea name="stepUpEvidence" rows={3} className="field font-mono text-[11px]" placeholder="Paste the short-lived signed evidence from the MFA authority" /></label><div className="rounded-lg border border-caution/30 bg-caution-soft/15 p-3 text-xs leading-5 text-caution">The approver is taken from the signed server-verified evidence. A name typed into this form cannot satisfy high-risk approval.</div><FormButtons busy={busy} onClose={onClose} submit="Create assignment" /></form></Overlay>;
}

function RoleRenewForm({ assignment, busy, onSubmit, onClose }: { assignment: PlatformRoleAssignment; busy: boolean; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; onClose: () => void }) {
  return <Overlay title={`Renew ${assignment.role_key}`} onClose={onClose}><form onSubmit={onSubmit} className="space-y-3"><div className="rounded-lg border border-info/30 bg-info-soft/15 p-3 text-xs leading-5 text-ink-2">This creates a new append-only version and revokes version {assignment.assignment_version} in the same transaction. The parent scope is preserved.</div><div className="grid grid-cols-2 gap-3"><Field label="New duration (minutes)" name="durationMinutes" type="number" defaultValue="60" min="1" /><div className="text-xs text-ink-2"><div>Risk tier</div><div className="field bg-paper-3/50">{assignment.risk_tier}</div></div></div>{isHighRiskRole(assignment) && <label className="block text-xs text-ink-2">Signed step-up MFA evidence<textarea name="stepUpEvidence" rows={3} required className="field font-mono text-[11px]" placeholder="Required for high-risk renewal" /></label>}<FormButtons busy={busy} onClose={onClose} submit="Renew assignment" /></form></Overlay>;
}

function RoleRevokeForm({ assignment, busy, onSubmit, onClose }: { assignment: PlatformRoleAssignment; busy: boolean; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; onClose: () => void }) {
  return <Overlay title={`Revoke ${assignment.role_key}`} onClose={onClose}><form onSubmit={onSubmit} className="space-y-3"><div className="rounded-lg border border-critical/30 bg-critical-soft/15 p-3 text-xs leading-5 text-critical">Revoke version {assignment.assignment_version} for <span className="font-mono">{assignment.principal_ref}</span>? The assignment is retained as historical evidence and will no longer be effective.</div>{isHighRiskRole(assignment) && <label className="block text-xs text-ink-2">Signed step-up MFA evidence<textarea name="stepUpEvidence" rows={3} required className="field font-mono text-[11px]" placeholder="Required for high-risk revocation" /></label>}<FormButtons busy={busy} onClose={onClose} submit="Revoke assignment" /></form></Overlay>;
}

function Overlay({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) { return <div className="fixed inset-0 z-modal grid place-items-center bg-black/40 p-4 backdrop-blur-sm"><div role="dialog" aria-modal="true" aria-label={title} className="panel-floating max-h-[90vh] w-full max-w-xl overflow-y-auto p-5"><div className="mb-4 flex items-center justify-between"><h2 className="text-lg font-semibold text-ink">{title}</h2><button type="button" onClick={onClose} className="rounded-lg p-1.5 text-mute hover:bg-paper-3 hover:text-ink" aria-label={`Close ${title}`}><X size={16} /></button></div>{children}</div></div>; }
function Field({ label, name, type = 'text', defaultValue, placeholder, min, max }: { label: string; name: string; type?: string; defaultValue?: string; placeholder?: string; min?: string; max?: string }) { return <label className="text-xs text-ink-2">{label}<input name={name} type={type} defaultValue={defaultValue} placeholder={placeholder} min={min} max={max} className="field" /></label>; }
function FormButtons({ busy, onClose, submit }: { busy: boolean; onClose: () => void; submit: string }) { return <div className="flex justify-end gap-2 pt-2"><button type="button" onClick={onClose} className="rounded-lg border border-rule px-3 py-2 text-sm text-ink-2 hover:bg-paper-3">Cancel</button><button type="submit" disabled={busy} className="rounded-lg bg-accent px-3 py-2 text-sm font-medium text-action-ink disabled:opacity-50">{busy ? 'Working…' : submit}</button></div>; }
