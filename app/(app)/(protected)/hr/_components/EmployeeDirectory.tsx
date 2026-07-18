'use client';

import { useState } from 'react';
import { useTranslations } from 'next-intl';
import type { EmployeeRow, LeaveRequestRow } from '@/hr/server';
import { T } from '@/components/i18n/T';
import { Empty } from '@/components/ui/Empty';
import { useToast } from '@/components/ui';
import { useHRContext } from './HRContext';

interface Props {
  employees: EmployeeRow[];
  requests: LeaveRequestRow[];
  statusBadge: (status: string) => React.ReactNode;
  leaveTypeThai: (type: string) => string;
  selectedEmployeeId: string | null;
  onSelectEmployee: (id: string | null) => void;
  onRefresh: () => Promise<void>;
}

interface QuotaForm {
  sick: number;
  annual: number;
  personal: number;
  reason: string;
}

export function EmployeeDirectory({
  employees,
  requests,
  statusBadge,
  leaveTypeThai,
  selectedEmployeeId,
  onSelectEmployee,
  onRefresh,
}: Props) {
  const { selectedHrId } = useHRContext();
  const t = useTranslations();
  const toast = useToast();
  const [searchTerm, setSearchTerm] = useState('');
  const [quotaForm, setQuotaForm] = useState<QuotaForm | null>(null);
  const [quotaSubmitting, setQuotaSubmitting] = useState(false);
  const [quotaSuccess, setQuotaSuccess] = useState<{ changes: { label: string; from: number; to: number }[] } | null>(null);

  const filteredEmployees = employees.filter((emp) =>
    emp.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    emp.employee_code.toLowerCase().includes(searchTerm.toLowerCase()) ||
    (emp.department ?? '').toLowerCase().includes(searchTerm.toLowerCase()) ||
    emp.position.toLowerCase().includes(searchTerm.toLowerCase())
  );

  const handleQuotaAdjust = async (emp: EmployeeRow) => {
    if (!selectedHrId) {
      toast.warning(t('hr.directory.selectOperator'));
      return;
    }
    if (!quotaForm) return;
    if (!quotaForm.reason.trim()) {
      toast.warning(t('hr.directory.reasonRequired'));
      return;
    }
    try {
      setQuotaSubmitting(true);
      const res = await fetch('/api/hr/employee/leave-quota', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          employeeId: emp.id,
          hrId: selectedHrId,
          totalSickLeave: quotaForm.sick,
          totalAnnualLeave: quotaForm.annual,
          totalPersonalLeave: quotaForm.personal,
          reason: quotaForm.reason,
        }),
      });
      const data = await res.json();
      if (data.success) {
        setQuotaSuccess({ changes: data.changes });
        setQuotaForm(null);
        await onRefresh();
        setTimeout(() => setQuotaSuccess(null), 8000);
      } else {
        toast.error(t('hr.directory.error', { error: String(data.error) }));
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(t('hr.directory.connectionError', { error: msg }));
    } finally {
      setQuotaSubmitting(false);
    }
  };

  const emp = employees.find((e) => e.id === selectedEmployeeId) || null;

  return (
    <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
      <div className="lg:col-span-4 bg-paper-2/30 border border-rule rounded-md p-5 space-y-4 flex flex-col">
        <h2 className="text-lg font-bold text-ink flex items-center gap-2 border-b border-rule pb-3 mb-2">
          <span>👥</span>
          <T id="hr.directory.title" />
        </h2>
        <div className="relative">
          <span className="absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none text-mute text-xs">
            🔍
          </span>
          <input
            type="text"
            placeholder={t('hr.directory.searchPlaceholder')}
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-paper border border-rule rounded-md pl-8 pr-8 py-2 text-sm text-ink placeholder-mute focus:outline-none focus:border-accent/40 focus:ring-1 focus:ring-accent/40 transition-all"
          />
          {searchTerm && (
            <button
              onClick={() => setSearchTerm('')}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 text-mute hover:text-ink text-xs font-bold px-1 py-0.5 rounded cursor-pointer"
            >
              ✕
            </button>
          )}
        </div>
        <div className="space-y-2.5 flex-1 max-h-[500px] overflow-y-auto pr-1">
          {filteredEmployees.length === 0 ? (
            <Empty
              title={<T id="hr.directory.empty" />}
              body={searchTerm ? 'Try a different search term.' : 'Employees will appear here once added.'}
            />
          ) : (
            filteredEmployees.map((e) => {
              const isSelected = selectedEmployeeId === e.id;
              return (
                <button
                  key={e.id}
                  onClick={() => { onSelectEmployee(e.id); setQuotaForm(null); setQuotaSuccess(null); }}
                  className={`w-full text-left p-3.5 rounded-md border transition-all flex flex-col gap-1 cursor-pointer ${
                    isSelected
                      ? 'bg-paper border-accent/40 shadow-md shadow-accent/20'
                      : 'bg-paper-2/40 border-rule hover:border-rule hover:bg-paper-2/60'
                  }`}
                >
                  <div className="font-bold text-sm text-ink">{e.name}</div>
                  <div className="text-xs text-ink-2 font-medium">{e.employee_code} • {e.position}</div>
                  <div className="text-[10px] text-accent mt-1 uppercase tracking-wider font-semibold">
                    <T id="hr.common.department" variant="compact" />: {e.department}
                  </div>
                </button>
              );
            })
          )}
        </div>
      </div>

      <div className="lg:col-span-8 space-y-6">
        {!emp ? (
          <div className="h-full min-h-[300px] flex flex-col items-center justify-center bg-paper-2/20 border border-rule border-dashed rounded-md p-8 text-center text-mute">
            <span className="text-4xl mb-3">👈</span>
            <h3 className="font-bold text-ink-2 text-sm"><T id="hr.directory.noSelectionTitle" /></h3>
            <p className="text-xs text-mute max-w-xs mt-1"><T id="hr.directory.noSelectionBody" /></p>
          </div>
        ) : (
          <EmployeeDetail
            emp={emp}
            requests={requests.filter((r) => String(r.employee_id) === emp.id)}
            statusBadge={statusBadge}
            leaveTypeThai={leaveTypeThai}
            quotaForm={quotaForm}
            setQuotaForm={setQuotaForm}
            quotaSubmitting={quotaSubmitting}
            quotaSuccess={quotaSuccess}
            onSave={handleQuotaAdjust}
          />
        )}
      </div>
    </div>
  );
}

interface DetailProps {
  emp: EmployeeRow;
  requests: LeaveRequestRow[];
  statusBadge: (status: string) => React.ReactNode;
  leaveTypeThai: (type: string) => string;
  quotaForm: QuotaForm | null;
  setQuotaForm: React.Dispatch<React.SetStateAction<QuotaForm | null>>;
  quotaSubmitting: boolean;
  quotaSuccess: { changes: { label: string; from: number; to: number }[] } | null;
  onSave: (emp: EmployeeRow) => Promise<void>;
}

function EmployeeDetail({
  emp,
  requests,
  statusBadge,
  leaveTypeThai,
  quotaForm,
  setQuotaForm,
  quotaSubmitting,
  quotaSuccess,
  onSave,
}: DetailProps) {
  const t = useTranslations();
  const sickRem = emp.total_sick_leave - emp.used_sick_leave;
  const annualRem = emp.total_annual_leave - emp.used_annual_leave;
  const personalRem = emp.total_personal_leave - emp.used_personal_leave;

  const quotaRows = [
    { key: 'sick' as const, emoji: '🤒', label: 'hr.common.typeSick', color: 'text-positive', ring: 'focus:ring-positive/40 focus:border-positive/40' },
    { key: 'annual' as const, emoji: '✈️', label: 'hr.common.typeAnnual', color: 'text-accent', ring: 'focus:ring-accent/40 focus:border-accent/40' },
    { key: 'personal' as const, emoji: '💼', label: 'hr.common.typePersonal', color: 'text-caution', ring: 'focus:ring-caution/40 focus:border-caution/40' },
  ];

  return (
    <div className="space-y-6">
      <div className="bg-paper-2/40 border border-rule p-6 rounded-md space-y-6">
        <div className="flex flex-col md:flex-row md:justify-between md:items-start border-b border-rule pb-5 gap-4">
          <div>
            <span className="px-2 py-0.5 text-[10px] font-bold rounded bg-accent text-accent border border-accent/40 uppercase tracking-wider">
              <T id="hr.common.employeeCode" variant="compact" />: {emp.employee_code}
            </span>
            <h2 className="text-2xl font-black text-ink mt-1.5">{emp.name}</h2>
            <p className="text-ink-2 text-sm mt-0.5">{emp.position} • {emp.department}</p>
          </div>
          <div className="bg-paper-2/60 border border-rule px-3.5 py-2 rounded-md text-center self-start">
            <div className="text-[10px] text-mute font-bold uppercase tracking-wider"><T id="hr.directory.systemRole" variant="compact" /></div>
            <div className="text-xs font-bold text-ink-2 mt-0.5 uppercase">{emp.role}</div>
          </div>
        </div>

        <div>
          <h3 className="text-xs font-bold text-ink-2 uppercase tracking-wider mb-2"><span>📋</span> <T id="hr.common.jobDescription" variant="compact" /></h3>
          <div className="bg-paper-2/50 border border-rule p-4 rounded-md text-sm text-ink-2 leading-relaxed italic">
            &ldquo;{emp.job_description || <T id="hr.common.noJobDescription" />}&rdquo;
          </div>
        </div>

        <div>
          <h3 className="text-xs font-bold text-ink-2 uppercase tracking-wider mb-3"><span>📊</span> <T id="hr.common.leaveBalance" variant="compact" /></h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <QuotaCard label={<><span>🤒</span> <T id="hr.common.typeSick" variant="compact" /></>} total={emp.total_sick_leave} rem={sickRem} color="bg-positive" />
            <QuotaCard label={<><span>✈️</span> <T id="hr.common.typeAnnual" variant="compact" /></>} total={emp.total_annual_leave} rem={annualRem} color="bg-accent" />
            <QuotaCard label={<><span>💼</span> <T id="hr.common.typePersonal" variant="compact" /></>} total={emp.total_personal_leave} rem={personalRem} color="bg-caution" />
          </div>
        </div>

        <div className="border-t border-rule pt-5">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-xs font-bold text-ink-2 uppercase tracking-wider"><span>⚙️</span> <T id="hr.directory.adjustQuota" variant="compact" /></h3>
            {!quotaForm ? (
              <button
                onClick={() => setQuotaForm({
                  sick: emp.total_sick_leave,
                  annual: emp.total_annual_leave,
                  personal: emp.total_personal_leave,
                  reason: '',
                })}
                className="px-3 py-1.5 text-xs font-bold rounded-lg bg-accent-strong hover:bg-accent-strong border border-accent/40 text-ink transition cursor-pointer flex items-center gap-1.5"
              >
                <span>✏️</span> <T id="hr.directory.editQuota" variant="compact" />
              </button>
            ) : (
              <button
                onClick={() => setQuotaForm(null)}
                className="px-3 py-1.5 text-xs font-bold rounded-lg bg-paper-2 hover:bg-paper-2 border border-rule text-ink-2 transition cursor-pointer"
              >
                <span>✕</span> <T id="hr.directory.cancel" variant="compact" />
              </button>
            )}
          </div>

          {quotaSuccess && (
            <div className="mb-4 bg-positive border border-positive/40 rounded-md p-4 space-y-2">
              <p className="text-positive font-bold text-sm flex items-center gap-2"><span>✅</span> <T id="hr.directory.quotaSuccess" /></p>
              {quotaSuccess.changes.map((c) => {
                const delta = c.to - c.from;
                return (
                  <div key={c.label} className="flex items-center gap-2 text-xs">
                    <span className="text-ink-2 font-semibold w-20">{c.label}</span>
                    <span className="text-ink-2">{c.from} → {c.to} <T id="hr.common.days" variant="compact" /></span>
                    <span className={`font-bold ml-1 ${delta > 0 ? 'text-positive' : 'text-critical'}`}>
                      {delta > 0 ? `▲ +${delta}` : `▼ ${delta}`}
                    </span>
                  </div>
                );
              })}
              <p className="text-positive text-[11px] mt-1"><span>📲</span> <T id="hr.directory.lineNotified" variant="compact" /></p>
            </div>
          )}

          {quotaForm && (
            <div className="space-y-4">
              <div className="grid grid-cols-1 gap-3">
                {quotaRows.map((row) => {
                  const original = row.key === 'sick'
                    ? emp.total_sick_leave
                    : row.key === 'annual'
                    ? emp.total_annual_leave
                    : emp.total_personal_leave;
                  const current = quotaForm[row.key];
                  const delta = current - original;
                  return (
                    <div key={row.key} className="flex items-center gap-3 bg-paper-2/50 border border-rule rounded-md px-4 py-3">
                      <span className="text-ink-2 text-xs font-bold w-24 shrink-0">{row.emoji} <T id={row.label} variant="compact" /></span>
                      <span className="text-mute text-xs w-16 shrink-0"><T id="hr.directory.previous" variant="compact" />: <span className="text-ink-2 font-semibold">{original}</span> <T id="hr.common.days" variant="compact" /></span>
                      <button
                        type="button"
                        onClick={() => setQuotaForm((prev) => prev ? { ...prev, [row.key]: Math.max(0, prev[row.key] - 1) } : prev)}
                        className="w-7 h-7 rounded-lg bg-critical-strong hover:bg-critical-strong border border-critical/40 text-critical font-bold text-base leading-none flex items-center justify-center transition cursor-pointer shrink-0"
                      >−</button>
                      <input
                        type="number"
                        min={0}
                        max={365}
                        value={current}
                        onChange={(e) => setQuotaForm((prev) => prev ? { ...prev, [row.key]: Math.max(0, parseInt(e.target.value, 10) || 0) } : prev)}
                        className={`w-16 text-center bg-paper border border-rule rounded-lg py-1 text-sm font-bold text-ink focus:outline-none focus:ring-1 ${row.ring} transition`}
                      />
                      <button
                        type="button"
                        onClick={() => setQuotaForm((prev) => prev ? { ...prev, [row.key]: prev[row.key] + 1 } : prev)}
                        className="w-7 h-7 rounded-lg bg-positive-strong hover:bg-positive-strong border border-positive/40 text-positive font-bold text-base leading-none flex items-center justify-center transition cursor-pointer shrink-0"
                      >+</button>
                      <span className="text-xs font-bold ml-1 shrink-0">
                        {delta === 0 ? <span className="text-mute">—</span>
                          : delta > 0 ? <span className="text-positive">▲ +{delta}</span>
                          : <span className="text-critical">▼ {delta}</span>}
                      </span>
                    </div>
                  );
                })}
              </div>

              <div>
                <label className="text-xs font-bold text-ink-2 uppercase tracking-wider block mb-1.5">
                  <span>📝</span> <T id="hr.directory.adjustmentReason" variant="compact" /> <span className="text-critical">*</span>
                </label>
                <textarea
                  rows={2}
                  placeholder={t('hr.directory.adjustmentPlaceholder')}
                  value={quotaForm.reason}
                  onChange={(e) => setQuotaForm((prev) => prev ? { ...prev, reason: e.target.value } : prev)}
                  className="w-full bg-paper border border-rule rounded-md px-4 py-2.5 text-sm text-ink placeholder-mute focus:outline-none focus:border-accent/40 focus:ring-1 focus:ring-accent/40 transition resize-none"
                />
              </div>

              <button
                onClick={() => onSave(emp)}
                disabled={quotaSubmitting || !quotaForm.reason.trim()}
                className="w-full py-2.5 rounded-md text-sm font-bold  from-accent-strong to-accent hover:from-accent hover:to-accent text-ink disabled:opacity-40 disabled:cursor-not-allowed transition-all shadow-lg shadow-accent/40 flex items-center justify-center gap-2 cursor-pointer"
              >
                {quotaSubmitting ? (
                  <><span className="animate-spin inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full" /><T id="hr.directory.saving" /></>
                ) : (
                  <><span>💾</span> <T id="hr.directory.saveAndNotify" /></>
                )}
              </button>
            </div>
          )}

          {!quotaForm && !quotaSuccess && (
            <p className="text-mute text-xs italic"><T id="hr.directory.editHint" /></p>
          )}
        </div>
      </div>

      <div className="bg-paper-2/40 border border-rule rounded-md p-6">
          <h3 className="text-sm font-bold text-ink border-b border-rule pb-3 mb-4 flex items-center gap-2">
            <span>📜</span> <T id="hr.directory.history" />
          </h3>
          {requests.length === 0 ? (
            <Empty
              title={<T id="hr.directory.noHistory" />}
              body="Submitted leave requests will appear here."
            />
          ) : (
          <div className="space-y-3 max-h-[300px] overflow-y-auto pr-1">
            {requests.map((req) => (
              <div key={req.id} className="bg-paper-2/45 border border-rule p-4 rounded-md flex flex-col md:flex-row md:items-center justify-between gap-3 text-xs">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-bold text-ink-2">{leaveTypeThai(req.leave_type)}</span>
                    <span className="text-mute font-bold">•</span>
                    <span className="text-ink font-bold">{req.days} <T id="hr.common.days" variant="compact" /></span>
                  </div>
                  <div className="text-ink-2 font-mono">{req.start_date} <T id="hr.common.to" variant="compact" /> {req.end_date}</div>
                  <div className="text-ink-2 italic"><T id="hr.common.reason" variant="compact" />: {req.reason || <T id="hr.common.noReason" variant="compact" />}</div>
                  {req.status === 'rejected' && req.reject_reason && (
                    <div className="text-critical font-medium"><T id="hr.common.rejectionReason" variant="compact" />: {req.reject_reason}</div>
                  )}
                </div>
                <div className="flex flex-col items-start md:items-end gap-1">
                  {statusBadge(req.status)}
                  {req.approved_by_name && (
                    <span className="text-[9px] text-mute"><T id="hr.common.by" variant="compact" />: {req.approved_by_name}</span>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function QuotaCard({ label, rem, total, color }: { label: React.ReactNode; total: number; rem: number; color: string }) {
  const pct = (rem / total) * 100;
  return (
    <div className="bg-paper-2/30 border border-rule p-3.5 rounded-md space-y-2">
      <div className="flex justify-between text-xs">
        <span className="font-bold text-ink-2">{label}</span>
        <span className="text-ink-2 font-extrabold">{rem} / {total} <T id="hr.common.days" variant="compact" /></span>
      </div>
      <div className="h-2 w-full bg-paper-2 rounded-full overflow-hidden">
        <div className={`h-full ${color} rounded-full`} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
