import 'server-only';
import type { ReactNode } from 'react';
import { notFound } from 'next/navigation';
import Link from 'next/link';
import { PageLayout } from '@/components/PageLayout';
import { T } from '@/components/i18n/TServer';
import type { SecondaryLocale } from '@/i18n/config';
import { getSecondaryLocale } from '@/server/locale';
import { getEmployee } from '@/hr/server';
import { listLeaveRequests } from '@/hr/server';

export const dynamic = 'force-dynamic';

export default async function EmployeeDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const locale = await getSecondaryLocale();
  const emp = await getEmployee(id);
  if (!emp) notFound();
  const requests = await listLeaveRequests({ employeeId: id });

  const sickRem = emp.total_sick_leave - emp.used_sick_leave;
  const annualRem = emp.total_annual_leave - emp.used_annual_leave;
  const personalRem = emp.total_personal_leave - emp.used_personal_leave;

  return (
    <PageLayout className="max-w-4xl">
      <div className="space-y-6">
        <Link
          href="/hr"
          className="text-sm text-accent hover:text-accent"
        >
          ← <T id="hr.common.backToDashboard" locale={locale} />
        </Link>

        <div className="bg-paper-2/40 border border-rule p-6 rounded-md space-y-4">
          <div className="flex items-start justify-between border-b border-rule pb-4">
            <div>
              <span className="px-2 py-0.5 text-[10px] font-bold rounded bg-accent text-accent border border-accent/40 uppercase tracking-wider">
                <T id="hr.common.employeeCode" locale={locale} variant="compact" />: {emp.employee_code}
              </span>
              <h1 className="text-3xl font-black text-ink mt-2">{emp.name}</h1>
              <p className="text-ink-2 text-sm mt-1">{emp.position} • {emp.department}</p>
            </div>
            <span className="text-xs font-bold text-ink-2 bg-paper-2/60 px-3 py-1 rounded-md border border-rule uppercase">
              {emp.role}
            </span>
          </div>

          <div className="bg-paper-2/50 border border-rule p-4 rounded-md">
            <h3 className="text-xs font-bold text-ink-2 uppercase tracking-wider mb-2"><span>📋</span> <T id="hr.common.jobDescription" locale={locale} variant="compact" /></h3>
            <p className="text-sm text-ink-2 leading-relaxed italic">
              &ldquo;{emp.job_description || <T id="hr.common.noJobDescription" locale={locale} />}&rdquo;
            </p>
          </div>

          <div>
            <h3 className="text-xs font-bold text-ink-2 uppercase tracking-wider mb-3"><span>📊</span> <T id="hr.common.leaveBalance" locale={locale} variant="compact" /></h3>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <Quota label={<><span>🤒</span> <T id="hr.common.typeSick" locale={locale} variant="compact" /></>} rem={sickRem} total={emp.total_sick_leave} color="bg-positive" text="text-positive" locale={locale} />
              <Quota label={<><span>✈️</span> <T id="hr.common.typeAnnual" locale={locale} variant="compact" /></>} rem={annualRem} total={emp.total_annual_leave} color="bg-accent" text="text-accent" locale={locale} />
              <Quota label={<><span>💼</span> <T id="hr.common.typePersonal" locale={locale} variant="compact" /></>} rem={personalRem} total={emp.total_personal_leave} color="bg-caution" text="text-caution" locale={locale} />
            </div>
          </div>
        </div>

        <div className="bg-paper-2/40 border border-rule rounded-md p-6">
          <h2 className="text-sm font-bold text-ink border-b border-rule pb-3 mb-4"><span>📜</span> <T id="hr.employeeDetail.history" locale={locale} /></h2>
          {requests.length === 0 ? (
            <p className="text-mute text-sm italic text-center py-4"><T id="hr.employeeDetail.noHistory" locale={locale} /></p>
          ) : (
            <div className="space-y-3">
              {requests.map((r) => (
                <Link
                  key={r.id}
                  href={`/hr/leave/${r.id}`}
                  className="block bg-paper-2/45 border border-rule p-4 rounded-md hover:border-accent/40 transition"
                >
                  <div className="flex justify-between items-start gap-3">
                    <div>
                      <div className="font-bold text-ink">
                        {r.leave_type === 'sick' ? <><span>🤒</span> <T id="hr.common.typeSick" locale={locale} /></> : r.leave_type === 'annual' ? <><span>✈️</span> <T id="hr.common.typeAnnual" locale={locale} /></> : <><span>💼</span> <T id="hr.common.typePersonal" locale={locale} /></>}
                      </div>
                      <div className="text-xs text-ink-2 font-mono mt-1">{r.start_date} <T id="hr.common.to" locale={locale} variant="compact" /> {r.end_date} ({r.days} <T id="hr.common.days" locale={locale} variant="compact" />)</div>
                      {r.reason && <div className="text-xs text-ink-2 italic mt-1"><T id="hr.common.reason" locale={locale} variant="compact" />: {r.reason}</div>}
                    </div>
                    <div>
                      {r.status === 'approved' && <span className="px-2 py-1 text-xs rounded-full bg-positive text-positive border border-positive/40"><T id="hr.common.statusApproved" locale={locale} variant="compact" /></span>}
                      {r.status === 'rejected' && <span className="px-2 py-1 text-xs rounded-full bg-critical text-critical border border-critical/40"><T id="hr.common.statusRejected" locale={locale} variant="compact" /></span>}
                      {r.status === 'pending' && <span className="px-2 py-1 text-xs rounded-full bg-caution text-caution border border-caution/40"><T id="hr.common.statusPending" locale={locale} variant="compact" /></span>}
                    </div>
                  </div>
                </Link>
              ))}
            </div>
          )}
        </div>
      </div>
    </PageLayout>
  );
}

function Quota({ label, rem, total, color, text, locale }: { label: ReactNode; rem: number; total: number; color: string; text: string; locale: SecondaryLocale }) {
  return (
    <div className="bg-paper-2/30 border border-rule p-3.5 rounded-md space-y-2">
      <div className="flex justify-between text-xs">
        <span className="font-bold text-ink-2">{label}</span>
        <span className={`${text} font-extrabold`}>{rem} / {total} <T id="hr.common.days" locale={locale} variant="compact" /></span>
      </div>
      <div className="h-2 w-full bg-paper-2 rounded-full overflow-hidden">
        <div className={`h-full ${color} rounded-full`} style={{ width: `${(rem / total) * 100}%` }} />
      </div>
    </div>
  );
}
