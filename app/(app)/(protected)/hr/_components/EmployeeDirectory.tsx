'use client';

import { useState } from 'react';
import type { EmployeeRow, LeaveRequestRow } from '@/hr/server';
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
  const [searchTerm, setSearchTerm] = useState('');
  const [quotaForm, setQuotaForm] = useState<QuotaForm | null>(null);
  const [quotaSubmitting, setQuotaSubmitting] = useState(false);
  const [quotaSuccess, setQuotaSuccess] = useState<{ changes: { label: string; from: number; to: number }[] } | null>(null);

  const filteredEmployees = employees.filter((emp) =>
    emp.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    emp.employee_code.toLowerCase().includes(searchTerm.toLowerCase()) ||
    emp.department.toLowerCase().includes(searchTerm.toLowerCase()) ||
    emp.position.toLowerCase().includes(searchTerm.toLowerCase())
  );

  const handleQuotaAdjust = async (emp: EmployeeRow) => {
    if (!selectedHrId) {
      alert('โปรดเลือก HR ผู้ดำเนินการก่อน');
      return;
    }
    if (!quotaForm) return;
    if (!quotaForm.reason.trim()) {
      alert('กรุณาระบุเหตุผลในการปรับสิทธิ์วันลา');
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
        alert('เกิดข้อผิดพลาด: ' + data.error);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      alert('เกิดข้อผิดพลาดในการเชื่อมต่อ: ' + msg);
    } finally {
      setQuotaSubmitting(false);
    }
  };

  const emp = employees.find((e) => e.id === selectedEmployeeId) || null;

  return (
    <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
      <div className="lg:col-span-4 bg-slate-900/30 border border-slate-800 rounded-2xl p-5 space-y-4 flex flex-col">
        <h2 className="text-lg font-bold text-slate-200 flex items-center gap-2 border-b border-slate-800 pb-3 mb-2">
          👥 รายชื่อพนักงาน
        </h2>
        <div className="relative">
          <span className="absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none text-slate-500 text-xs">
            🔍
          </span>
          <input
            type="text"
            placeholder="ค้นหาชื่อ, รหัส หรือแผนก..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-slate-950 border border-slate-800 rounded-xl pl-8 pr-8 py-2 text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 transition-all"
          />
          {searchTerm && (
            <button
              onClick={() => setSearchTerm('')}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-200 text-xs font-bold px-1 py-0.5 rounded cursor-pointer"
            >
              ✕
            </button>
          )}
        </div>
        <div className="space-y-2.5 flex-1 max-h-[500px] overflow-y-auto pr-1">
          {filteredEmployees.length === 0 ? (
            <p className="text-slate-500 text-xs italic text-center py-8">ไม่พบข้อมูลพนักงาน</p>
          ) : (
            filteredEmployees.map((e) => {
              const isSelected = selectedEmployeeId === e.id;
              return (
                <button
                  key={e.id}
                  onClick={() => { onSelectEmployee(e.id); setQuotaForm(null); setQuotaSuccess(null); }}
                  className={`w-full text-left p-3.5 rounded-xl border transition-all flex flex-col gap-1 cursor-pointer ${
                    isSelected
                      ? 'bg-slate-900 border-indigo-500 shadow-md shadow-indigo-950/20'
                      : 'bg-slate-900/40 border-slate-800 hover:border-slate-700 hover:bg-slate-900/60'
                  }`}
                >
                  <div className="font-bold text-sm text-slate-200">{e.name}</div>
                  <div className="text-xs text-slate-400 font-medium">{e.employee_code} • {e.position}</div>
                  <div className="text-[10px] text-indigo-400 mt-1 uppercase tracking-wider font-semibold">
                    แผนก: {e.department}
                  </div>
                </button>
              );
            })
          )}
        </div>
      </div>

      <div className="lg:col-span-8 space-y-6">
        {!emp ? (
          <div className="h-full min-h-[300px] flex flex-col items-center justify-center bg-slate-900/20 border border-slate-800 border-dashed rounded-2xl p-8 text-center text-slate-500">
            <span className="text-4xl mb-3">👈</span>
            <h3 className="font-bold text-slate-400 text-sm">ยังไม่ได้เลือกพนักงาน</h3>
            <p className="text-xs text-slate-500 max-w-xs mt-1">โปรดเลือกรายชื่อพนักงานจากแถบซ้ายมือเพื่อดูข้อมูลส่วนตัว ขอบข่ายหน้าที่งาน และสิทธิ์วันลาคงเหลือ</p>
          </div>
        ) : (
          <EmployeeDetail
            emp={emp}
            requests={requests.filter((r) => r.employee_id === emp.id)}
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
  const sickRem = emp.total_sick_leave - emp.used_sick_leave;
  const annualRem = emp.total_annual_leave - emp.used_annual_leave;
  const personalRem = emp.total_personal_leave - emp.used_personal_leave;

  const quotaRows = [
    { key: 'sick' as const, emoji: '🤒', label: 'ลาป่วย', color: 'text-emerald-400', ring: 'focus:ring-emerald-500/40 focus:border-emerald-500' },
    { key: 'annual' as const, emoji: '✈️', label: 'ลาพักร้อน', color: 'text-indigo-400', ring: 'focus:ring-indigo-500/40 focus:border-indigo-500' },
    { key: 'personal' as const, emoji: '💼', label: 'ลากิจ', color: 'text-amber-400', ring: 'focus:ring-amber-500/40 focus:border-amber-500' },
  ];

  return (
    <div className="space-y-6">
      <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl space-y-6">
        <div className="flex flex-col md:flex-row md:justify-between md:items-start border-b border-slate-800 pb-5 gap-4">
          <div>
            <span className="px-2 py-0.5 text-[10px] font-bold rounded bg-indigo-500/20 text-indigo-400 border border-indigo-500/30 uppercase tracking-wider">
              รหัสพนักงาน: {emp.employee_code}
            </span>
            <h2 className="text-2xl font-black text-slate-100 mt-1.5">{emp.name}</h2>
            <p className="text-slate-400 text-sm mt-0.5">{emp.position} • {emp.department}</p>
          </div>
          <div className="bg-slate-950/60 border border-slate-800 px-3.5 py-2 rounded-xl text-center self-start">
            <div className="text-[10px] text-slate-500 font-bold uppercase tracking-wider">บทบาทระบบ</div>
            <div className="text-xs font-bold text-slate-300 mt-0.5 uppercase">{emp.role}</div>
          </div>
        </div>

        <div>
          <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider mb-2">📋 ขอบข่ายหน้าที่งาน (Job Description)</h3>
          <div className="bg-slate-950/40 border border-slate-800 p-4 rounded-xl text-sm text-slate-300 leading-relaxed italic">
            &ldquo;{emp.job_description || 'ไม่มีรายละเอียดขอบข่ายหน้าที่งานระบุไว้'}&rdquo;
          </div>
        </div>

        <div>
          <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider mb-3">📊 สิทธิ์วันลาคงเหลือ</h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <QuotaCard label="🤒 ลาป่วย" used={emp.used_sick_leave} total={emp.total_sick_leave} rem={sickRem} color="bg-emerald-500" />
            <QuotaCard label="✈️ ลาพักร้อน" used={emp.used_annual_leave} total={emp.total_annual_leave} rem={annualRem} color="bg-indigo-500" />
            <QuotaCard label="💼 ลากิจ" used={emp.used_personal_leave} total={emp.total_personal_leave} rem={personalRem} color="bg-amber-500" />
          </div>
        </div>

        <div className="border-t border-slate-800 pt-5">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider">⚙️ ปรับโควตาวันลา (HR)</h3>
            {!quotaForm ? (
              <button
                onClick={() => setQuotaForm({
                  sick: emp.total_sick_leave,
                  annual: emp.total_annual_leave,
                  personal: emp.total_personal_leave,
                  reason: '',
                })}
                className="px-3 py-1.5 text-xs font-bold rounded-lg bg-indigo-600/70 hover:bg-indigo-600 border border-indigo-500/30 text-white transition cursor-pointer flex items-center gap-1.5"
              >
                ✏️ แก้ไขโควตา
              </button>
            ) : (
              <button
                onClick={() => setQuotaForm(null)}
                className="px-3 py-1.5 text-xs font-bold rounded-lg bg-slate-700 hover:bg-slate-600 border border-slate-600 text-slate-300 transition cursor-pointer"
              >
                ✕ ยกเลิก
              </button>
            )}
          </div>

          {quotaSuccess && (
            <div className="mb-4 bg-emerald-500/10 border border-emerald-500/30 rounded-xl p-4 space-y-2">
              <p className="text-emerald-400 font-bold text-sm flex items-center gap-2">✅ ปรับโควตาสำเร็จแล้ว!</p>
              {quotaSuccess.changes.map((c) => {
                const delta = c.to - c.from;
                return (
                  <div key={c.label} className="flex items-center gap-2 text-xs">
                    <span className="text-slate-300 font-semibold w-20">{c.label}</span>
                    <span className="text-slate-400">{c.from} → {c.to} วัน</span>
                    <span className={`font-bold ml-1 ${delta > 0 ? 'text-emerald-400' : 'text-rose-400'}`}>
                      {delta > 0 ? `▲ +${delta}` : `▼ ${delta}`}
                    </span>
                  </div>
                );
              })}
              <p className="text-emerald-500/70 text-[11px] mt-1">📲 แจ้งเตือนถูกส่งไปยัง LINE ของพนักงานแล้ว</p>
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
                    <div key={row.key} className="flex items-center gap-3 bg-slate-950/40 border border-slate-800 rounded-xl px-4 py-3">
                      <span className="text-slate-300 text-xs font-bold w-24 shrink-0">{row.emoji} {row.label}</span>
                      <span className="text-slate-500 text-xs w-16 shrink-0">เดิม: <span className="text-slate-300 font-semibold">{original}</span> วัน</span>
                      <button
                        type="button"
                        onClick={() => setQuotaForm((prev) => prev ? { ...prev, [row.key]: Math.max(0, prev[row.key] - 1) } : prev)}
                        className="w-7 h-7 rounded-lg bg-rose-600/30 hover:bg-rose-600/60 border border-rose-500/30 text-rose-300 font-bold text-base leading-none flex items-center justify-center transition cursor-pointer shrink-0"
                      >−</button>
                      <input
                        type="number"
                        min={0}
                        max={365}
                        value={current}
                        onChange={(e) => setQuotaForm((prev) => prev ? { ...prev, [row.key]: Math.max(0, parseInt(e.target.value, 10) || 0) } : prev)}
                        className={`w-16 text-center bg-slate-900 border border-slate-700 rounded-lg py-1 text-sm font-bold text-slate-100 focus:outline-none focus:ring-1 ${row.ring} transition`}
                      />
                      <button
                        type="button"
                        onClick={() => setQuotaForm((prev) => prev ? { ...prev, [row.key]: prev[row.key] + 1 } : prev)}
                        className="w-7 h-7 rounded-lg bg-emerald-600/30 hover:bg-emerald-600/60 border border-emerald-500/30 text-emerald-300 font-bold text-base leading-none flex items-center justify-center transition cursor-pointer shrink-0"
                      >+</button>
                      <span className="text-xs font-bold ml-1 shrink-0">
                        {delta === 0 ? <span className="text-slate-600">—</span>
                          : delta > 0 ? <span className="text-emerald-400">▲ +{delta}</span>
                          : <span className="text-rose-400">▼ {delta}</span>}
                      </span>
                    </div>
                  );
                })}
              </div>

              <div>
                <label className="text-xs font-bold text-slate-400 uppercase tracking-wider block mb-1.5">
                  📝 เหตุผลในการปรับ <span className="text-rose-400">*</span>
                </label>
                <textarea
                  rows={2}
                  placeholder="เช่น ปรับตามนโยบายใหม่ประจำปี / พนักงานได้รับสิทธิ์เพิ่มพิเศษ..."
                  value={quotaForm.reason}
                  onChange={(e) => setQuotaForm((prev) => prev ? { ...prev, reason: e.target.value } : prev)}
                  className="w-full bg-slate-950 border border-slate-700 rounded-xl px-4 py-2.5 text-sm text-slate-200 placeholder-slate-600 focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500/40 transition resize-none"
                />
              </div>

              <button
                onClick={() => onSave(emp)}
                disabled={quotaSubmitting || !quotaForm.reason.trim()}
                className="w-full py-2.5 rounded-xl text-sm font-bold bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 text-white disabled:opacity-40 disabled:cursor-not-allowed transition-all shadow-lg shadow-indigo-950/40 flex items-center justify-center gap-2 cursor-pointer"
              >
                {quotaSubmitting ? (
                  <><span className="animate-spin inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full" />กำลังบันทึก...</>
                ) : (
                  <>💾 บันทึกและแจ้งเตือนพนักงาน</>
                )}
              </button>
            </div>
          )}

          {!quotaForm && !quotaSuccess && (
            <p className="text-slate-600 text-xs italic">คลิก &quot;แก้ไขโควตา&quot; เพื่อปรับสิทธิ์วันลาสำหรับพนักงานคนนี้</p>
          )}
        </div>
      </div>

      <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6">
        <h3 className="text-sm font-bold text-slate-200 border-b border-slate-800 pb-3 mb-4 flex items-center gap-2">
          📜 ประวัติการยื่นใบลาทั้งหมด
        </h3>
        {requests.length === 0 ? (
          <p className="text-slate-500 text-sm italic py-4 text-center">พนักงานคนนี้ยังไม่มีประวัติการส่งใบลาหยุดงาน</p>
        ) : (
          <div className="space-y-3 max-h-[300px] overflow-y-auto pr-1">
            {requests.map((req) => (
              <div key={req.id} className="bg-slate-950/45 border border-slate-800 p-4 rounded-xl flex flex-col md:flex-row md:items-center justify-between gap-3 text-xs">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-bold text-slate-300">{leaveTypeThai(req.leave_type)}</span>
                    <span className="text-slate-500 font-bold">•</span>
                    <span className="text-slate-200 font-bold">{req.days} วัน</span>
                  </div>
                  <div className="text-slate-400 font-mono">{req.start_date} ถึง {req.end_date}</div>
                  <div className="text-slate-400 italic">เหตุผล: {req.reason || 'ไม่ได้ระบุ'}</div>
                  {req.status === 'rejected' && req.reject_reason && (
                    <div className="text-rose-400 font-medium">เหตุผลปฏิเสธ: {req.reject_reason}</div>
                  )}
                </div>
                <div className="flex flex-col items-start md:items-end gap-1">
                  {statusBadge(req.status)}
                  {req.approved_by_name && (
                    <span className="text-[9px] text-slate-500">โดย: {req.approved_by_name}</span>
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

function QuotaCard({ label, rem, total, color }: { label: string; used: number; total: number; rem: number; color: string }) {
  const pct = (rem / total) * 100;
  return (
    <div className="bg-slate-950/30 border border-slate-800 p-3.5 rounded-xl space-y-2">
      <div className="flex justify-between text-xs">
        <span className="font-bold text-slate-300">{label}</span>
        <span className="text-slate-300 font-extrabold">{rem} / {total} วัน</span>
      </div>
      <div className="h-2 w-full bg-slate-800 rounded-full overflow-hidden">
        <div className={`h-full ${color} rounded-full`} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
