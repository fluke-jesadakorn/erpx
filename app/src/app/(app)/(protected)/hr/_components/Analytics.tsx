'use client';

import { useMemo } from 'react';
import type { LeaveRequestRow, LeaveStats, DeptStat, EmployeeRow } from '@folio-lib/hr/server';

const THAI_MONTHS = [
  'มกราคม', 'กุมภาพันธ์', 'มีนาคม', 'เมษายน', 'พฤษภาคม', 'มิถุนายน',
  'กรกฎาคม', 'สิงหาคม', 'กันยายน', 'ตุลาคม', 'พฤศจิกายน', 'ธันวาคม',
];

interface Props {
  stats: LeaveStats;
  requests: LeaveRequestRow[];
  employees: EmployeeRow[];
  deptStats: DeptStat[];
  maxDeptDays: number;
}

export function Analytics({ stats, requests, employees, deptStats, maxDeptDays }: Props) {
  const monthlyTrend = useMemo(() => {
    const now = new Date();
    const months: { key: string; label: string; sick: number; annual: number; personal: number }[] = [];
    for (let i = 5; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
      const label = `${THAI_MONTHS[d.getMonth()].slice(0, 3)} ${d.getFullYear()}`;
      months.push({ key, label, sick: 0, annual: 0, personal: 0 });
    }
    for (const req of requests) {
      const created = req.created_at.slice(0, 7);
      const slot = months.find((m) => m.key === created);
      if (slot) {
        if (req.leave_type === 'sick') slot.sick += req.days;
        else if (req.leave_type === 'annual') slot.annual += req.days;
        else if (req.leave_type === 'personal') slot.personal += req.days;
      }
    }
    return months;
  }, [requests]);

  const maxMonthlyDays = useMemo(
    () => Math.max(...monthlyTrend.map((m) => m.sick + m.annual + m.personal), 1),
    [monthlyTrend],
  );

  const leaveTypeCounts = useMemo(() => {
    const approved = requests.filter((r) => r.status === 'approved');
    const sick = approved.filter((r) => r.leave_type === 'sick').reduce((a, b) => a + b.days, 0);
    const annual = approved.filter((r) => r.leave_type === 'annual').reduce((a, b) => a + b.days, 0);
    const personal = approved.filter((r) => r.leave_type === 'personal').reduce((a, b) => a + b.days, 0);
    const total = sick + annual + personal || 1;
    return { sick, annual, personal, total };
  }, [requests]);

  const burnoutRisk = useMemo(
    () => employees.filter((e) => e.used_annual_leave === 0 && e.total_annual_leave > 0),
    [employees],
  );

  const sickPct = Math.round((leaveTypeCounts.sick / leaveTypeCounts.total) * 360);
  const annualPct = Math.round((leaveTypeCounts.annual / leaveTypeCounts.total) * 360);
  const conicGrad = `conic-gradient(
    #10b981 0deg ${sickPct}deg,
    #6366f1 ${sickPct}deg ${sickPct + annualPct}deg,
    #f59e0b ${sickPct + annualPct}deg 360deg
  )`;

  return (
    <div className="space-y-8">
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="bg-slate-900/60 backdrop-blur-md border border-slate-800 p-5 rounded-2xl flex flex-col justify-between">
          <span className="text-slate-400 text-xs font-bold uppercase tracking-wider">คำขอทั้งหมด</span>
          <span className="text-3xl font-black text-slate-100 mt-2">{stats.total}</span>
        </div>
        <div className="bg-amber-500/10 border border-amber-500/20 p-5 rounded-2xl flex flex-col justify-between">
          <span className="text-amber-400 text-xs font-bold uppercase tracking-wider">รออนุมัติ</span>
          <span className="text-3xl font-black text-amber-300 mt-2">{stats.pending}</span>
        </div>
        <div className="bg-emerald-500/10 border border-emerald-500/20 p-5 rounded-2xl flex flex-col justify-between">
          <span className="text-emerald-400 text-xs font-bold uppercase tracking-wider">อนุมัติแล้ว</span>
          <span className="text-3xl font-black text-emerald-300 mt-2">{stats.approved}</span>
        </div>
        <div className="bg-rose-500/10 border border-rose-500/20 p-5 rounded-2xl flex flex-col justify-between">
          <span className="text-rose-400 text-xs font-bold uppercase tracking-wider">ปฏิเสธแล้ว</span>
          <span className="text-3xl font-black text-rose-300 mt-2">{stats.rejected}</span>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 bg-slate-900/40 border border-slate-800 p-6 rounded-2xl">
          <h2 className="text-base font-bold text-slate-200 mb-1 flex items-center gap-2">
            📈 แนวโน้มการลา (6 เดือนล่าสุด)
          </h2>
          <p className="text-xs text-slate-500 mb-5">นับจากวันที่ส่งคำขอ (ทุกสถานะ) หน่วย: วัน</p>
          <div className="flex items-end gap-3 h-40">
            {monthlyTrend.map((m) => {
              const total = m.sick + m.annual + m.personal;
              const pct = (total / maxMonthlyDays) * 100;
              const sickH = total > 0 ? (m.sick / total) * pct : 0;
              const annualH = total > 0 ? (m.annual / total) * pct : 0;
              const personalH = total > 0 ? (m.personal / total) * pct : 0;
              return (
                <div key={m.key} className="flex-1 flex flex-col items-center gap-1 group">
                  <span className="text-[10px] text-slate-400 font-semibold opacity-0 group-hover:opacity-100 transition-opacity">
                    {total > 0 ? `${total}ว` : '-'}
                  </span>
                  <div
                    className="w-full flex flex-col-reverse rounded-t-lg overflow-hidden"
                    style={{ height: `${Math.max(pct, total > 0 ? 4 : 0)}%`, minHeight: total > 0 ? '4px' : '0' }}
                  >
                    <div style={{ height: `${sickH}%` }} className="bg-emerald-500/70 w-full transition-all duration-500" />
                    <div style={{ height: `${annualH}%` }} className="bg-indigo-500/70 w-full transition-all duration-500" />
                    <div style={{ height: `${personalH}%` }} className="bg-amber-500/70 w-full transition-all duration-500" />
                  </div>
                  <span className="text-[10px] text-slate-500 text-center leading-tight">{m.label}</span>
                </div>
              );
            })}
          </div>
          <div className="flex gap-4 mt-4 pt-4 border-t border-slate-800">
            <span className="flex items-center gap-1.5 text-xs text-emerald-300">
              <span className="w-2.5 h-2.5 rounded-sm bg-emerald-500/70 inline-block" /> ลาป่วย
            </span>
            <span className="flex items-center gap-1.5 text-xs text-indigo-300">
              <span className="w-2.5 h-2.5 rounded-sm bg-indigo-500/70 inline-block" /> ลาพักร้อน
            </span>
            <span className="flex items-center gap-1.5 text-xs text-amber-300">
              <span className="w-2.5 h-2.5 rounded-sm bg-amber-500/70 inline-block" /> ลากิจ
            </span>
          </div>
        </div>

        <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl flex flex-col items-center justify-center">
          <h2 className="text-base font-bold text-slate-200 mb-1 self-start">🍰 สัดส่วนประเภทการลา</h2>
          <p className="text-xs text-slate-500 mb-5 self-start">เฉพาะที่อนุมัติแล้ว (วันสะสม)</p>
          <div
            className="w-36 h-36 rounded-full shadow-2xl shadow-indigo-950/50"
            style={{ background: conicGrad }}
          />
          <div className="mt-5 space-y-2 w-full">
            {[
              { label: 'ลาป่วย', days: leaveTypeCounts.sick, color: 'bg-emerald-500', text: 'text-emerald-300' },
              { label: 'ลาพักร้อน', days: leaveTypeCounts.annual, color: 'bg-indigo-500', text: 'text-indigo-300' },
              { label: 'ลากิจ', days: leaveTypeCounts.personal, color: 'bg-amber-500', text: 'text-amber-300' },
            ].map((item) => (
              <div key={item.label} className="flex items-center justify-between text-xs">
                <span className="flex items-center gap-1.5 font-semibold text-slate-300">
                  <span className={`w-2.5 h-2.5 rounded-sm ${item.color} inline-block opacity-80`} />
                  {item.label}
                </span>
                <span className={`font-extrabold ${item.text}`}>
                  {item.days} วัน ({Math.round((item.days / leaveTypeCounts.total) * 100)}%)
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl">
          <h2 className="text-base font-bold text-slate-200 mb-1 flex items-center gap-2">
            🏢 วันลาตามแผนก (อนุมัติแล้ว)
          </h2>
          <p className="text-xs text-slate-500 mb-5">วันลาสะสมทุกประเภทที่ได้รับการอนุมัติ</p>
          {deptStats.length === 0 ? (
            <p className="text-slate-500 text-sm italic text-center py-8">ยังไม่มีข้อมูล</p>
          ) : (
            <div className="space-y-4">
              {deptStats.map((dept) => {
                const pct = (dept.total_days / maxDeptDays) * 100;
                return (
                  <div key={dept.department} className="space-y-1.5">
                    <div className="flex justify-between text-xs">
                      <span className="font-semibold text-slate-300">{dept.department}</span>
                      <span className="text-purple-400 font-bold">{dept.total_days} วัน</span>
                    </div>
                    <div className="h-2.5 w-full bg-slate-800 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 rounded-full transition-all duration-700"
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl">
          <h2 className="text-base font-bold text-slate-200 mb-1 flex items-center gap-2">
            🔥 ความเสี่ยง Burnout
          </h2>
          <p className="text-xs text-slate-500 mb-5">พนักงานที่ยังไม่ได้ใช้วันพักร้อนเลย (0/&gt;0 วัน)</p>
          {burnoutRisk.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-8 text-center">
              <span className="text-3xl mb-2">✅</span>
              <p className="text-emerald-400 font-semibold text-sm">พนักงานทุกคนใช้วันพักร้อนแล้ว</p>
              <p className="text-slate-500 text-xs mt-1">ไม่พบความเสี่ยง Burnout</p>
            </div>
          ) : (
            <div className="space-y-2.5 max-h-[240px] overflow-y-auto pr-1">
              {burnoutRisk.map((emp) => (
                <div
                  key={emp.id}
                  className="bg-rose-950/30 border border-rose-500/25 px-4 py-3 rounded-xl flex items-center justify-between gap-3"
                >
                  <div>
                    <div className="font-bold text-sm text-rose-200">{emp.name}</div>
                    <div className="text-[11px] text-rose-400/70 mt-0.5">{emp.department} • {emp.position}</div>
                  </div>
                  <div className="text-right shrink-0">
                    <div className="text-xs font-bold text-rose-400">0 / {emp.total_annual_leave} วัน</div>
                    <div className="text-[10px] text-rose-500/60 mt-0.5">ลาพักร้อนสะสม</div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
