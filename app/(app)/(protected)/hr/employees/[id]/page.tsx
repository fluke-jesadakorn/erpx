import 'server-only';
import { notFound } from 'next/navigation';
import Link from 'next/link';
import { getEmployee } from '@/hr/server';
import { listLeaveRequests } from '@/hr/server';

export const dynamic = 'force-dynamic';

export default async function EmployeeDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const emp = await getEmployee(id);
  if (!emp) notFound();
  const requests = await listLeaveRequests({ employeeId: id });

  const sickRem = emp.total_sick_leave - emp.used_sick_leave;
  const annualRem = emp.total_annual_leave - emp.used_annual_leave;
  const personalRem = emp.total_personal_leave - emp.used_personal_leave;

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100 p-6 md:p-12 font-sans">
      <div className="max-w-4xl mx-auto space-y-6">
        <Link
          href="/hr"
          className="text-sm text-indigo-400 hover:text-indigo-300"
        >
          ← กลับไป HR Dashboard
        </Link>

        <div className="bg-slate-900/40 border border-slate-800 p-6 rounded-2xl space-y-4">
          <div className="flex items-start justify-between border-b border-slate-800 pb-4">
            <div>
              <span className="px-2 py-0.5 text-[10px] font-bold rounded bg-indigo-500/20 text-indigo-400 border border-indigo-500/30 uppercase tracking-wider">
                รหัสพนักงาน: {emp.employee_code}
              </span>
              <h1 className="text-3xl font-black text-slate-100 mt-2">{emp.name}</h1>
              <p className="text-slate-400 text-sm mt-1">{emp.position} • {emp.department}</p>
            </div>
            <span className="text-xs font-bold text-slate-300 bg-slate-950/60 px-3 py-1 rounded-xl border border-slate-800 uppercase">
              {emp.role}
            </span>
          </div>

          <div className="bg-slate-950/40 border border-slate-800 p-4 rounded-xl">
            <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider mb-2">📋 Job Description</h3>
            <p className="text-sm text-slate-300 leading-relaxed italic">
              &ldquo;{emp.job_description || 'ไม่มีรายละเอียดขอบข่ายหน้าที่งานระบุไว้'}&rdquo;
            </p>
          </div>

          <div>
            <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider mb-3">📊 สิทธิ์วันลาคงเหลือ</h3>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <Quota label="🤒 ลาป่วย" rem={sickRem} total={emp.total_sick_leave} color="bg-emerald-500" text="text-emerald-300" />
              <Quota label="✈️ ลาพักร้อน" rem={annualRem} total={emp.total_annual_leave} color="bg-indigo-500" text="text-indigo-300" />
              <Quota label="💼 ลากิจ" rem={personalRem} total={emp.total_personal_leave} color="bg-amber-500" text="text-amber-300" />
            </div>
          </div>
        </div>

        <div className="bg-slate-900/40 border border-slate-800 rounded-2xl p-6">
          <h2 className="text-sm font-bold text-slate-200 border-b border-slate-800 pb-3 mb-4">📜 ประวัติการลาทั้งหมด</h2>
          {requests.length === 0 ? (
            <p className="text-slate-500 text-sm italic text-center py-4">ยังไม่มีประวัติการส่งใบลาหยุดงาน</p>
          ) : (
            <div className="space-y-3">
              {requests.map((r) => (
                <Link
                  key={r.id}
                  href={`/hr/leave/${r.id}`}
                  className="block bg-slate-950/45 border border-slate-800 p-4 rounded-xl hover:border-indigo-500/40 transition"
                >
                  <div className="flex justify-between items-start gap-3">
                    <div>
                      <div className="font-bold text-slate-200">
                        {r.leave_type === 'sick' ? '🤒 ลาป่วย' : r.leave_type === 'annual' ? '✈️ ลาพักร้อน' : '💼 ลากิจ'}
                      </div>
                      <div className="text-xs text-slate-400 font-mono mt-1">{r.start_date} ถึง {r.end_date} ({r.days} วัน)</div>
                      {r.reason && <div className="text-xs text-slate-400 italic mt-1">เหตุผล: {r.reason}</div>}
                    </div>
                    <div>
                      {r.status === 'approved' && <span className="px-2 py-1 text-xs rounded-full bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">อนุมัติแล้ว</span>}
                      {r.status === 'rejected' && <span className="px-2 py-1 text-xs rounded-full bg-rose-500/20 text-rose-400 border border-rose-500/30">ปฏิเสธแล้ว</span>}
                      {r.status === 'pending' && <span className="px-2 py-1 text-xs rounded-full bg-amber-500/20 text-amber-400 border border-amber-500/30">รออนุมัติ</span>}
                    </div>
                  </div>
                </Link>
              ))}
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

function Quota({ label, rem, total, color, text }: { label: string; rem: number; total: number; color: string; text: string }) {
  return (
    <div className="bg-slate-950/30 border border-slate-800 p-3.5 rounded-xl space-y-2">
      <div className="flex justify-between text-xs">
        <span className="font-bold text-slate-300">{label}</span>
        <span className={`${text} font-extrabold`}>{rem} / {total} วัน</span>
      </div>
      <div className="h-2 w-full bg-slate-800 rounded-full overflow-hidden">
        <div className={`h-full ${color} rounded-full`} style={{ width: `${(rem / total) * 100}%` }} />
      </div>
    </div>
  );
}
