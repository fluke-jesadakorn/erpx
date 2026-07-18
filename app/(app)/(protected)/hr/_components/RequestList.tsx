'use client';

import { useState } from 'react';
import type { LeaveRequestRow, LeaveStats, DeptStat } from '@/hr/server';
import { useHRContext } from './HRContext';
import { Empty } from '@/components/ui/Empty';
import { Modal, useToast } from '@/components/ui';

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

interface RejectState {
  requestId: string;
  reason: string;
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
  const toast = useToast();
  const [submittingId, setSubmittingId] = useState<string | null>(null);
  const [showExportMenu, setShowExportMenu] = useState(false);
  const [rejecting, setRejecting] = useState<RejectState | null>(null);

  const submitDecision = async (requestId: string, action: 'approve' | 'reject', reason: string) => {
    try {
      setSubmittingId(requestId);
      const res = await fetch('/api/hr/leave', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          requestId,
          action,
          hrId: selectedHrId,
          rejectReason: action === 'reject' ? reason : undefined,
        }),
      });
      const data = await res.json();
      if (data.success) {
        await onRefresh();
        toast.success(action === 'approve' ? 'อนุมัติคำขอลาเรียบร้อย' : 'ปฏิเสธคำขอลาเรียบร้อย');
      } else {
        toast.error('เกิดข้อผิดพลาด: ' + data.error);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error('เกิดข้อผิดพลาดในการเชื่อมต่อ: ' + msg);
    } finally {
      setSubmittingId(null);
    }
  };

  const handleDecision = async (requestId: string, action: 'approve' | 'reject') => {
    if (!selectedHrId) {
      toast.warning('โปรดเลือกผู้ใช้ HR ที่จะอนุมัติงานก่อน');
      return;
    }
    if (action === 'reject') {
      setRejecting({ requestId, reason: '' });
      return;
    }
    await submitDecision(requestId, action, '');
  };

  const submitReject = async () => {
    if (!rejecting) return;
    if (!rejecting.reason.trim()) {
      toast.warning('จำเป็นต้องระบุเหตุผลในการปฏิเสธคำขอลา');
      return;
    }
    const { requestId, reason } = rejecting;
    setRejecting(null);
    await submitDecision(requestId, 'reject', reason.trim());
  };

  return (
    <>
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 md:gap-6">
        <div className="bg-paper-2/60 backdrop-blur-md border border-rule p-5 rounded-md flex flex-col justify-between">
          <span className="text-ink-2 text-xs font-bold uppercase tracking-wider">คำขอลาทั้งหมด</span>
          <span className="text-3xl font-black text-ink mt-2">{stats.total}</span>
        </div>
        <div className="bg-caution border border-caution/40 p-5 rounded-md flex flex-col justify-between">
          <span className="text-caution text-xs font-bold uppercase tracking-wider">รออนุมัติ</span>
          <span className="text-3xl font-black text-caution mt-2">{stats.pending}</span>
        </div>
        <div className="bg-positive border border-positive/40 p-5 rounded-md flex flex-col justify-between">
          <span className="text-positive text-xs font-bold uppercase tracking-wider">อนุมัติแล้ว</span>
          <span className="text-3xl font-black text-positive mt-2">{stats.approved}</span>
        </div>
        <div className="bg-critical border border-critical/40 p-5 rounded-md flex flex-col justify-between">
          <span className="text-critical text-xs font-bold uppercase tracking-wider">ปฏิเสธแล้ว</span>
          <span className="text-3xl font-black text-critical mt-2">{stats.rejected}</span>
        </div>
      </div>

      <div className="bg-paper-2/40 border border-rule p-6 rounded-md">
        <h2 className="text-lg font-bold text-ink mb-4 flex items-center gap-2">
          📊 สถิติวันลาสะสมตามแผนก (เฉพาะที่อนุมัติแล้ว)
        </h2>
        {deptStats.length === 0 ? (
          <p className="text-mute text-sm italic">ยังไม่มีข้อมูลวันลาที่ได้รับการอนุมัติ</p>
        ) : (
          <div className="space-y-4">
            {deptStats.map((dept) => {
              const pct = (dept.total_days / maxDeptDays) * 100;
              return (
                <div key={dept.department} className="space-y-1.5">
                  <div className="flex justify-between text-sm">
                    <span className="font-semibold text-ink-2">{dept.department}</span>
                    <span className="text-accent font-bold">{dept.total_days} วัน</span>
                  </div>
                  <div className="h-2.5 w-full bg-paper-2 rounded-full overflow-hidden">
                    <div
                      className="h-full  from-accent via-accent to-accent rounded-full transition-all duration-500"
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div className="bg-paper-2/30 border border-rule rounded-md overflow-hidden shadow-2xl">
        <div className="px-6 py-4 border-b border-rule bg-paper-2/50 flex justify-between items-center gap-3">
          <h2 className="text-lg font-bold text-ink">
            📋 รายการขอลาหยุดทั้งหมด
          </h2>
          <div className="flex items-center gap-2">
            <div className="relative">
              <button
                onClick={() => setShowExportMenu((v) => !v)}
                className="px-3 py-1.5 text-xs font-semibold rounded-lg bg-positive-strong hover:bg-positive-strong border border-positive/60 text-ink transition cursor-pointer flex items-center gap-1.5"
              >
                ⬇️ ส่งออก CSV
              </button>
              {showExportMenu && (
                <div className="absolute right-0 top-full mt-1.5 z-sticky bg-paper-2 border border-rule rounded-md shadow-2xl overflow-hidden min-w-[180px]">
                  <button
                    onClick={() => { setShowExportMenu(false); onExport('this-month'); }}
                    className="w-full text-left px-4 py-2.5 text-sm text-ink hover:bg-paper-2 transition cursor-pointer"
                  >
                    เดือนนี้ ({currentMonthLabel})
                  </button>
                  <button
                    onClick={() => { setShowExportMenu(false); onExport('last-month'); }}
                    className="w-full text-left px-4 py-2.5 text-sm text-ink hover:bg-paper-2 transition cursor-pointer"
                  >
                    เดือนที่แล้ว
                  </button>
                  <button
                    onClick={() => { setShowExportMenu(false); onExport('all'); }}
                    className="w-full text-left px-4 py-2.5 text-sm text-ink hover:bg-paper-2 transition cursor-pointer border-t border-rule"
                  >
                    ทั้งหมด
                  </button>
                </div>
              )}
            </div>
            <button
              onClick={onRefresh}
              className="px-3 py-1.5 text-xs font-semibold rounded-lg bg-accent-strong hover:bg-accent-strong border border-accent/40 text-ink transition cursor-pointer"
            >
              โหลดข้อมูลใหม่
            </button>
          </div>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead>
              <tr className="bg-paper-2/50 text-ink-2 text-xs font-bold uppercase tracking-wider border-b border-rule">
                <th className="px-6 py-4">พนักงาน / แผนก</th>
                <th className="px-6 py-4">ประเภทการลา</th>
                <th className="px-6 py-4">ระยะเวลาที่ลา</th>
                <th className="px-6 py-4">จำนวนวัน</th>
                <th className="px-6 py-4">เหตุผลการลา</th>
                <th className="px-6 py-4">สถานะ</th>
                <th className="px-6 py-4 text-right">ดำเนินการ</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-rule text-sm">
              {requests.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-6">
                    <Empty
                      title="ยังไม่มีประวัติการส่งคำขอลาหยุดงาน"
                      body="คำขอลาใหม่จะปรากฏที่นี่เมื่อพนักงานส่งคำขอ"
                    />
                  </td>
                </tr>
              ) : (
                requests.map((req) => (
                  <tr key={req.id} className="hover:bg-paper-2/20 transition-colors">
                    <td className="px-6 py-4">
                      <button
                        onClick={() => onSelectEmployee(String(req.employee_id))}
                        className="font-bold text-accent hover:text-accent hover:underline text-left focus:outline-none cursor-pointer"
                      >
                        {req.employee_name}
                      </button>
                      <div className="text-xs text-ink-2 mt-0.5">{req.employee_code} • {req.department}</div>
                    </td>
                    <td className="px-6 py-4 font-medium text-ink-2">{leaveTypeThai(req.leave_type)}</td>
                    <td className="px-6 py-4 text-ink-2 font-mono text-xs">{req.start_date} ถึง {req.end_date}</td>
                    <td className="px-6 py-4 text-ink font-bold">{req.days} วัน</td>
                    <td className="px-6 py-4 text-ink-2 italic max-w-xs truncate" title={req.reason || ''}>
                      {req.reason || 'ไม่ได้ระบุเหตุผล'}
                    </td>
                    <td className="px-6 py-4">
                      <div className="flex flex-col gap-1 items-start">
                        {statusBadge(req.status)}
                        {req.approved_by_name && (
                          <span className="text-[10px] text-mute">โดย: {req.approved_by_name}</span>
                        )}
                        {req.status === 'rejected' && req.reject_reason && (
                          <span className="text-[10px] text-critical mt-1 max-w-[150px] break-words">
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
                            className="px-3 py-1.5 rounded-lg text-xs font-bold bg-positive-strong hover:bg-positive-strong border border-positive/40 text-ink disabled:opacity-40 transition cursor-pointer"
                          >
                            อนุมัติ
                          </button>
                          <button
                            onClick={() => handleDecision(req.id, 'reject')}
                            disabled={submittingId !== null}
                            className="px-3 py-1.5 rounded-lg text-xs font-bold bg-critical-strong hover:bg-critical-strong border border-critical/40 text-ink disabled:opacity-40 transition cursor-pointer"
                          >
                            ปฏิเสธ
                          </button>
                        </div>
                      ) : (
                        <span className="text-xs text-mute italic">เสร็จสิ้นแล้ว</span>
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      <Modal
        open={rejecting !== null}
        onClose={() => (submittingId !== null ? null : setRejecting(null))}
        title="ปฏิเสธคำขอลา"
        subtitle="โปรดระบุเหตุผลในการปฏิเสธคำขอลางานนี้ (จำเป็น)"
        tone="rose"
        width="md"
        footer={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setRejecting(null)}
              disabled={submittingId !== null}
              className="px-3 py-1.5 rounded-lg text-xs font-bold bg-paper-2 hover:bg-paper-2 border border-rule text-ink-2 disabled:opacity-40 transition cursor-pointer"
            >
              ยกเลิก
            </button>
            <button
              type="button"
              onClick={submitReject}
              disabled={submittingId !== null}
              className="px-3 py-1.5 rounded-lg text-xs font-bold bg-critical-strong hover:bg-critical-strong border border-critical/40 text-ink disabled:opacity-40 transition cursor-pointer"
            >
              ส่งการปฏิเสธ
            </button>
          </div>
        }
      >
        <textarea
          rows={4}
          autoFocus
          value={rejecting?.reason ?? ''}
          onChange={(e) => setRejecting((prev) => (prev ? { ...prev, reason: e.target.value } : prev))}
          placeholder="ระบุเหตุผลในการปฏิเสธ..."
          className="w-full bg-paper border border-rule rounded-md px-3 py-2 text-sm text-ink placeholder-mute focus:outline-none focus:border-critical/40 focus:ring-1 focus:ring-critical/40 transition resize-none"
        />
      </Modal>
    </>
  );
}
