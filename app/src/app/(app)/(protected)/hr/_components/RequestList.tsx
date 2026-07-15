'use client';

import { useState } from 'react';
import type { LeaveRequestRow, LeaveStats, DeptStat } from '@folio-lib/hr/server';
import { useHRContext } from './HRContext';

interface Props {
  requests: LeaveRequestRow[];
  stats: LeaveStats;
  deptStats: DeptStat[];
  maxDeptDays: number;
  statusBadge: (status: string) => React.ReactNode;
  leaveTypeThai: (type: string) => string;
  currentMonthLabel: string;
  onSelectEmployee: (id: string) => void;
  onExport: (period: 'this-month' | 'last-month' | 'all') => void;
  onRefresh: () => Promise<void>;
}

export function RequestList({
  requests,
  stats,
  deptStats,
  maxDeptDays,
  statusBadge,
  leaveTypeThai,
  currentMonthLabel,
  onSelectEmployee,
  onExport,
  onRefresh,
}: Props) {
  const { selectedHrId } = useHRContext();
  const [submittingId, setSubmittingId] = useState<string | null>(null);
  const [showExportMenu, setShowExportMenu] = useState(false);

  const handleDecision = async (requestId: string, action: 'approve' | 'reject') => {
    if (!selectedHrId) {
      alert('โปรดเลือกผู้ใช้ HR ที่จะอนุมัติงานก่อน');
      return;
    }
    let rejectReason = '';
    if (action === 'reject') {
      const reasonInput = prompt('โปรดระบุเหตุผลในการปฏิเสธคำขอลางานนี้ (จำเป็น):');
      if (reasonInput === null) return;
      if (!reasonInput.trim()) {
        alert('จำเป็นต้องระบุเหตุผลในการปฏิเสธคำขอลา');
        return;
      }
      rejectReason = reasonInput.trim();
    }
    try {
      setSubmittingId(requestId);
      const res = await fetch('/api/hr/leave', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          requestId,
          action,
          hrId: selectedHrId,
          rejectReason: action === 'reject' ? rejectReason : undefined,
        }),
      });
      const data = await res.json();
      if (data.success) {
        await onRefresh();
      } else {
        alert('เกิดข้อผิดพลาด: ' + data.error);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      alert('เกิดข้อผิดพลาดในการเชื่อมต่อ: ' + msg);
    } finally {
      setSubmittingId(null);
    }
  };

  return (
    <>
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 md:gap-6">
        <div className="bg-slate-900/60 backdrop-blur-md border border-slate-800 p-5 rounded-2xl flex flex-col justify-between">
          <span className="text-slate-400 text-xs font-bold uppercase tracking-wider">คำขอลาทั้งหมด</span>
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

      <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl">
        <h2 className="text-lg font-bold text-slate-200 mb-4 flex items-center gap-2">
          📊 สถิติวันลาสะสมตามแผนก (เฉพาะที่อนุมัติแล้ว)
        </h2>
        {deptStats.length === 0 ? (
          <p className="text-slate-500 text-sm italic">ยังไม่มีข้อมูลวันลาที่ได้รับการอนุมัติ</p>
        ) : (
          <div className="space-y-4">
            {deptStats.map((dept) => {
              const pct = (dept.total_days / maxDeptDays) * 100;
              return (
                <div key={dept.department} className="space-y-1.5">
                  <div className="flex justify-between text-sm">
                    <span className="font-semibold text-slate-300">{dept.department}</span>
                    <span className="text-indigo-400 font-bold">{dept.total_days} วัน</span>
                  </div>
                  <div className="h-2.5 w-full bg-slate-800 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 rounded-full transition-all duration-500"
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div className="bg-slate-900/30 border border-slate-800 rounded-2xl overflow-hidden shadow-2xl">
        <div className="px-6 py-4 border-b border-slate-800 bg-slate-900/50 flex justify-between items-center gap-3">
          <h2 className="text-lg font-bold text-slate-200">
            📋 รายการขอลาหยุดทั้งหมด
          </h2>
          <div className="flex items-center gap-2">
            <div className="relative">
              <button
                onClick={() => setShowExportMenu((v) => !v)}
                className="px-3 py-1.5 text-xs font-semibold rounded-lg bg-emerald-700/70 hover:bg-emerald-700 border border-emerald-600/30 text-white transition cursor-pointer flex items-center gap-1.5"
              >
                ⬇️ ส่งออก CSV
              </button>
              {showExportMenu && (
                <div className="absolute right-0 top-full mt-1.5 z-30 bg-slate-800 border border-slate-700 rounded-xl shadow-2xl overflow-hidden min-w-[180px]">
                  <button
                    onClick={() => { setShowExportMenu(false); onExport('this-month'); }}
                    className="w-full text-left px-4 py-2.5 text-sm text-slate-200 hover:bg-slate-700 transition cursor-pointer"
                  >
                    เดือนนี้ ({currentMonthLabel})
                  </button>
                  <button
                    onClick={() => { setShowExportMenu(false); onExport('last-month'); }}
                    className="w-full text-left px-4 py-2.5 text-sm text-slate-200 hover:bg-slate-700 transition cursor-pointer"
                  >
                    เดือนที่แล้ว
                  </button>
                  <button
                    onClick={() => { setShowExportMenu(false); onExport('all'); }}
                    className="w-full text-left px-4 py-2.5 text-sm text-slate-200 hover:bg-slate-700 transition cursor-pointer border-t border-slate-700"
                  >
                    ทั้งหมด
                  </button>
                </div>
              )}
            </div>
            <button
              onClick={onRefresh}
              className="px-3 py-1.5 text-xs font-semibold rounded-lg bg-indigo-600/80 hover:bg-indigo-600 border border-indigo-500/20 text-white transition cursor-pointer"
            >
              โหลดข้อมูลใหม่
            </button>
          </div>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-slate-950/50 text-slate-400 text-xs font-bold uppercase tracking-wider border-b border-slate-800">
                <th className="px-6 py-4">พนักงาน / แผนก</th>
                <th className="px-6 py-4">ประเภทการลา</th>
                <th className="px-6 py-4">ระยะเวลาที่ลา</th>
                <th className="px-6 py-4">จำนวนวัน</th>
                <th className="px-6 py-4">เหตุผลการลา</th>
                <th className="px-6 py-4">สถานะ</th>
                <th className="px-6 py-4 text-right">ดำเนินการ</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800 text-sm">
              {requests.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-6 py-10 text-center text-slate-500 italic">
                    ยังไม่มีประวัติการส่งคำขอลาหยุดงาน
                  </td>
                </tr>
              ) : (
                requests.map((req) => (
                  <tr key={req.id} className="hover:bg-slate-900/20 transition-colors">
                    <td className="px-6 py-4">
                      <button
                        onClick={() => onSelectEmployee(req.employee_id)}
                        className="font-bold text-indigo-400 hover:text-indigo-300 hover:underline text-left focus:outline-none cursor-pointer"
                      >
                        {req.employee_name}
                      </button>
                      <div className="text-xs text-slate-400 mt-0.5">{req.employee_code} • {req.department}</div>
                    </td>
                    <td className="px-6 py-4 font-medium text-slate-300">{leaveTypeThai(req.leave_type)}</td>
                    <td className="px-6 py-4 text-slate-300 font-mono text-xs">{req.start_date} ถึง {req.end_date}</td>
                    <td className="px-6 py-4 text-slate-200 font-bold">{req.days} วัน</td>
                    <td className="px-6 py-4 text-slate-400 italic max-w-xs truncate" title={req.reason || ''}>
                      {req.reason || 'ไม่ได้ระบุเหตุผล'}
                    </td>
                    <td className="px-6 py-4">
                      <div className="flex flex-col gap-1 items-start">
                        {statusBadge(req.status)}
                        {req.approved_by_name && (
                          <span className="text-[10px] text-slate-500">โดย: {req.approved_by_name}</span>
                        )}
                        {req.status === 'rejected' && req.reject_reason && (
                          <span className="text-[10px] text-rose-400 mt-1 max-w-[150px] break-words">
                            เหตุผล: {req.reject_reason}
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-6 py-4 text-right">
                      {req.status === 'pending' ? (
                        <div className="flex justify-end gap-2">
                          <button
                            onClick={() => handleDecision(req.id, 'approve')}
                            disabled={submittingId !== null}
                            className="px-3 py-1.5 rounded-lg text-xs font-bold bg-emerald-600/80 hover:bg-emerald-600 border border-emerald-500/20 text-white disabled:opacity-40 transition cursor-pointer"
                          >
                            อนุมัติ
                          </button>
                          <button
                            onClick={() => handleDecision(req.id, 'reject')}
                            disabled={submittingId !== null}
                            className="px-3 py-1.5 rounded-lg text-xs font-bold bg-rose-600/80 hover:bg-rose-600 border border-rose-500/20 text-white disabled:opacity-40 transition cursor-pointer"
                          >
                            ปฏิเสธ
                          </button>
                        </div>
                      ) : (
                        <span className="text-xs text-slate-500 italic">เสร็จสิ้นแล้ว</span>
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}
