import 'server-only';
import { notFound } from 'next/navigation';
import Link from 'next/link';
import { findLeaveRequestById } from '@/hr/server';

export const dynamic = 'force-dynamic';

export default async function LeaveDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const req = await findLeaveRequestById(id);
  if (!req) notFound();

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100 p-6 md:p-12 font-sans">
      <div className="max-w-3xl mx-auto space-y-6">
        <Link
          href="/hr"
          className="text-sm text-indigo-400 hover:text-indigo-300"
        >
          ← กลับไป HR Dashboard
        </Link>

        <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl space-y-4">
          <div className="flex items-start justify-between border-b border-slate-800 pb-4">
            <div>
              <h1 className="text-2xl font-bold text-slate-100">{req.leave_type === 'sick' ? '🤒 ลาป่วย' : req.leave_type === 'annual' ? '✈️ ลาพักร้อน' : '💼 ลากิจ'}</h1>
              <p className="text-slate-400 text-sm mt-1">คำขอลาหยุดงาน</p>
            </div>
            <div>
              {req.status === 'approved' && <span className="px-3 py-1 text-xs font-semibold rounded-full bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">อนุมัติแล้ว</span>}
              {req.status === 'rejected' && <span className="px-3 py-1 text-xs font-semibold rounded-full bg-rose-500/20 text-rose-400 border border-rose-500/30">ปฏิเสธแล้ว</span>}
              {req.status === 'pending' && <span className="px-3 py-1 text-xs font-semibold rounded-full bg-amber-500/20 text-amber-400 border border-amber-500/30">รออนุมัติ</span>}
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Field label="พนักงาน" value={`${req.employee_name} (${req.employee_code})`} />
            <Field label="แผนก" value={req.department} />
            <Field label="ตำแหน่ง" value={req.position} />
            <Field label="จำนวนวัน" value={`${req.days} วัน`} />
            <Field label="วันที่เริ่ม" value={req.start_date} />
            <Field label="วันที่สิ้นสุด" value={req.end_date} />
            <Field label="วันที่ส่งคำขอ" value={new Date(req.created_at).toISOString().slice(0, 10)} />
            <Field
              label="ผู้อนุมัติ"
              value={req.approved_by_name || (req.approved_by ? req.approved_by : '—')}
            />
          </div>

          {req.reason && (
            <div>
              <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider mb-1">เหตุผลการลา</h3>
              <p className="bg-slate-950/40 border border-slate-800 p-4 rounded-xl text-sm text-slate-200 italic">
                {req.reason}
              </p>
            </div>
          )}

          {req.status === 'rejected' && req.reject_reason && (
            <div>
              <h3 className="text-xs font-bold text-rose-400 uppercase tracking-wider mb-1">เหตุผลการปฏิเสธ</h3>
              <p className="bg-rose-950/30 border border-rose-500/30 p-4 rounded-xl text-sm text-rose-200">
                {req.reject_reason}
              </p>
            </div>
          )}
        </div>

        <Link
          href={`/hr/employees/${req.employee_id}`}
          className="inline-block text-sm text-indigo-400 hover:text-indigo-300"
        >
          → ดูประวัติทั้งหมดของ {req.employee_name}
        </Link>
      </div>
    </main>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="space-y-1">
      <div className="text-[10px] uppercase tracking-wider text-slate-500 font-bold">{label}</div>
      <div className="text-sm text-slate-200">{value}</div>
    </div>
  );
}
