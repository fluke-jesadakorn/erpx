import 'server-only';
import type { ReactNode } from 'react';
import { notFound, redirect } from 'next/navigation';
import Link from 'next/link';
import { headers } from 'next/headers';
import { PageLayout } from '@/components/PageLayout';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { findLeaveRequestById } from '@/hr/server';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { loadActor } from '@/server/guard';
import { approveLeaveAction, rejectLeaveAction } from './_actions';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

export default async function LeaveDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const locale = await getSecondaryLocale();
  const req = await findLeaveRequestById(id);
  if (!req) notFound();

  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal/hr/leave/' + id, { headers: h as unknown as HeadersInit }),
  );
  if (!out) redirect('/login');
  const canDecide =
    hasPermission(out.session, PERM.hr.leave.approve) ||
    hasPermission(out.session, PERM.hr.leave.reject);
  const actor = await loadActor();

  return (
    <PageLayout className="max-w-3xl">
      <div className="space-y-6">
        <Link
          href="/hr/leave"
          className="text-sm text-accent hover:text-accent"
        >
          ← <T id="hr.common.backToDashboard" locale={locale} />
        </Link>

        <div className="bg-paper-2/40 border border-rule p-6 rounded-md space-y-4">
          <div className="flex items-start justify-between border-b border-rule pb-4">
            <div>
              <h1 className="text-2xl font-bold text-ink">{req.leave_type === 'sick' ? <><span>🤒</span> <T id="hr.common.typeSick" locale={locale} /></> : req.leave_type === 'annual' ? <><span>✈️</span> <T id="hr.common.typeAnnual" locale={locale} /></> : <><span>💼</span> <T id="hr.common.typePersonal" locale={locale} /></>}</h1>
              <p className="text-ink-2 text-sm mt-1"><T id="hr.leaveDetail.requestTitle" locale={locale} /></p>
            </div>
            <div>
              {req.status === 'approved' && <span data-testid="status-badge" data-status="approved" className="px-3 py-1 text-xs font-semibold rounded-full bg-positive text-positive border border-positive/40"><T id="hr.common.statusApproved" locale={locale} variant="compact" /></span>}
              {req.status === 'rejected' && <span data-testid="status-badge" data-status="rejected" className="px-3 py-1 text-xs font-semibold rounded-full bg-critical text-critical border border-critical/40"><T id="hr.common.statusRejected" locale={locale} variant="compact" /></span>}
              {req.status === 'pending' && <span data-testid="status-badge" data-status="pending" className="px-3 py-1 text-xs font-semibold rounded-full bg-caution text-caution border border-caution/40"><T id="hr.common.statusPending" locale={locale} variant="compact" /></span>}
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Field label={<T id="hr.leaveDetail.employee" locale={locale} variant="compact" />} value={`${req.employee_name} (${req.employee_code})`} />
            <Field label={<T id="hr.common.department" locale={locale} variant="compact" />} value={req.department ?? '—'} />
            <Field label={<T id="hr.common.position" locale={locale} variant="compact" />} value={req.position} />
            <Field label={<T id="hr.leaveDetail.numberOfDays" locale={locale} variant="compact" />} value={<>{req.days} <T id="hr.common.days" locale={locale} variant="compact" /></>} />
            <Field label={<T id="hr.leaveDetail.startDate" locale={locale} variant="compact" />} value={req.start_date} />
            <Field label={<T id="hr.leaveDetail.endDate" locale={locale} variant="compact" />} value={req.end_date} />
            <Field label={<T id="hr.leaveDetail.submittedDate" locale={locale} variant="compact" />} value={new Date(req.created_at).toISOString().slice(0, 10)} />
            <Field
              label={<T id="hr.leaveDetail.approver" locale={locale} variant="compact" />}
              value={req.approved_by_name || (req.approved_by ? String(req.approved_by) : '—')}
            />
          </div>

          {req.reason && (
            <div>
              <h3 className="text-xs font-bold text-ink-2 uppercase tracking-wider mb-1"><T id="hr.leaveDetail.leaveReason" locale={locale} variant="compact" /></h3>
              <p className="bg-paper-2/50 border border-rule p-4 rounded-md text-sm text-ink italic">
                {req.reason}
              </p>
            </div>
          )}

          {req.status === 'rejected' && req.reject_reason && (
            <div>
              <h3 className="text-xs font-bold text-critical uppercase tracking-wider mb-1"><T id="hr.leaveDetail.rejectionReason" locale={locale} variant="compact" /></h3>
              <p className="bg-critical-strong border border-critical/40 p-4 rounded-md text-sm text-critical">
                {req.reject_reason}
              </p>
            </div>
          )}
        </div>

        {canDecide && req.status === 'pending' ? (
          <div className="bg-paper-2/40 border border-rule rounded-md p-6 space-y-4" data-testid="decision-panel">
            <h2 className="text-sm font-bold text-ink border-b border-rule pb-3">Decision</h2>
            <div className="flex flex-wrap gap-3">
              <form action={approveLeaveAction}>
                <input type="hidden" name="waybillId" value={req.id} />
                <button
                  type="submit"
                  data-testid="approve-leave"
                  className="px-4 py-2 text-sm font-bold rounded-md bg-positive-strong border border-positive/40 text-ink hover:bg-positive transition"
                >
                  ✅ Approve
                </button>
              </form>
              <form action={rejectLeaveAction} className="flex flex-wrap items-center gap-2">
                <input type="hidden" name="waybillId" value={req.id} />
                <input
                  type="text"
                  name="reason"
                  required
                  minLength={5}
                  data-testid="reject-reason"
                  placeholder="Rejection reason (≥5 chars)"
                  className="px-3 py-2 text-sm rounded-md border border-rule bg-paper text-ink"
                />
                <button
                  type="submit"
                  data-testid="reject-leave"
                  className="px-4 py-2 text-sm font-bold rounded-md bg-critical-strong border border-critical/40 text-ink hover:bg-critical transition"
                >
                  ❌ Reject
                </button>
              </form>
            </div>
          </div>
        ) : null}

        {!canDecide && actor ? (
          <NoPermissionView
            kind="stage_locked"
            actor={actor as never}
            attemptedPath={`/hr/leave/${req.id}`}
            reason="hr:leave:approve required."
          />
        ) : null}

        <Link
          href={`/hr/employees/${req.employee_id}`}
          className="inline-block text-sm text-accent hover:text-accent"
        >
          → <T id="hr.leaveDetail.viewHistory" locale={locale} /> {req.employee_name}
        </Link>
      </div>
    </PageLayout>
  );
}

function Field({ label, value }: { label: ReactNode; value: ReactNode }) {
  return (
    <div className="space-y-1">
      <div className="text-[10px] uppercase tracking-wider text-mute font-bold">{label}</div>
      <div className="text-sm text-ink">{value}</div>
    </div>
  );
}