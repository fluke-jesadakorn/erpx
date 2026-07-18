'use client';

import type {
  HRUserOption,
  LeaveRequestRow,
  LeaveStats,
  DeptStat,
  EmployeeRow,
} from '@/hr/server';
import { useState, useEffect, useMemo, useCallback } from 'react';
import { HRProvider, useHRContext } from './HRContext';
import { RequestList } from './RequestList';
import { EmployeeDirectory } from './EmployeeDirectory';
import { LeaveCalendar } from './LeaveCalendar';
import { Analytics } from './Analytics';
import { PageLayout } from '@/components/PageLayout';

interface Props {
  actorName: string;
  hrUsers: HRUserOption[];
  requests: LeaveRequestRow[];
  employees: EmployeeRow[];
  stats: LeaveStats;
  deptStats: DeptStat[];
}

type Tab = 'requests' | 'employees' | 'calendar' | 'analytics';

const THAI_MONTHS = [
  'มกราคม', 'กุมภาพันธ์', 'มีนาคม', 'เมษายน', 'พฤษภาคม', 'มิถุนายน',
  'กรกฎาคม', 'สิงหาคม', 'กันยายน', 'ตุลาคม', 'พฤศจิกายน', 'ธันวาคม',
];

function leaveTypeThai(type: string): string {
  if (type === 'sick') return '🤒 ลาป่วย';
  if (type === 'annual') return '✈️ ลาพักร้อน';
  if (type === 'personal') return '💼 ลากิจ';
  return type;
}

function statusBadge(status: string) {
  if (status === 'approved')
    return <span className="px-2 py-1 text-xs font-semibold rounded-full bg-positive text-positive border border-positive/40">อนุมัติแล้ว</span>;
  if (status === 'rejected')
    return <span className="px-2 py-1 text-xs font-semibold rounded-full bg-critical text-critical border border-critical/40">ปฏิเสธแล้ว</span>;
  return <span className="px-2 py-1 text-xs font-semibold rounded-full bg-caution text-caution border border-caution/40">รออนุมัติ</span>;
}

function HRDashboardInner(props: Props) {
  const [requests, setRequests] = useState(props.requests);
  const [employees, setEmployees] = useState(props.employees);
  const [stats, setStats] = useState(props.stats);
  const [deptStats, setDeptStats] = useState(props.deptStats);
  const [activeTab, setActiveTab] = useState<Tab>('requests');
  const [selectedEmployeeId, setSelectedEmployeeId] = useState<string | null>(null);
  const { selectedHrId, setSelectedHrId } = useHRContext();
  const [tooltipReq, setTooltipReq] = useState<{ req: LeaveRequestRow; x: number; y: number } | null>(null);

  const refresh = useCallback(async () => {
    const res = await fetch('/api/hr');
    const data = await res.json();
    if (data.success) {
      setRequests(data.requests);
      setStats(data.stats);
      setDeptStats(data.deptStats);
      setEmployees(data.employees || []);
    }
  }, []);

  useEffect(() => {
    const handler = () => setTooltipReq(null);
    window.addEventListener('click', handler);
    return () => window.removeEventListener('click', handler);
  }, []);

  const handleExport = (period: 'this-month' | 'last-month' | 'all') => {
    const now = new Date();
    if (period === 'all') {
      window.open('/api/hr/export', '_blank');
    } else {
      const d = period === 'this-month'
        ? new Date(now.getFullYear(), now.getMonth(), 1)
        : new Date(now.getFullYear(), now.getMonth() - 1, 1);
      const month = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
      window.open(`/api/hr/export?month=${month}`, '_blank');
    }
  };

  const maxDeptDays = deptStats.length > 0 ? Math.max(...deptStats.map(d => d.total_days)) : 1;

  const currentMonthLabel = useMemo(() => {
    const d = new Date();
    return `${THAI_MONTHS[d.getMonth()]} ${d.getFullYear() + 543}`;
  }, []);

  return (
    <PageLayout width="wide">
      {tooltipReq && (
        <div
          className="fixed z-fixed bg-paper-2 border border-rule text-ink text-xs px-3 py-2 rounded-md shadow-2xl pointer-events-none max-w-xs"
          style={{ top: tooltipReq.y + 12, left: tooltipReq.x + 8 }}
        >
          <div className="font-bold text-sm text-accent">{tooltipReq.req.employee_name}</div>
          <div className="mt-0.5 text-ink-2">{leaveTypeThai(tooltipReq.req.leave_type)}</div>
          <div className="text-mute mt-0.5 font-mono">{tooltipReq.req.start_date} → {tooltipReq.req.end_date}</div>
          <div className="text-ink-2 font-bold mt-0.5">{tooltipReq.req.days} วัน</div>
        </div>
      )}

      <div className="panel-elevated mb-7 flex flex-col gap-4 p-5 md:flex-row md:items-center md:justify-between sm:p-6">
        <div>
          <h1 className="page-title text-ink">
            HR Leave Management Portal
          </h1>
          <p className="text-ink-2 text-sm mt-1">
            ระบบตรวจสอบสิทธิ์ สถิติการลา และพิจารณาอนุมัติคำขอหยุดงานสำหรับฝ่ายบุคคล
          </p>
        </div>
        <div className="glass-input flex items-center gap-3 px-4 py-2.5 self-start md:self-auto">
          <span className="text-xs text-accent font-bold uppercase tracking-wider">สลับบัญชี HR:</span>
          <select
            value={selectedHrId}
            onChange={(e) => setSelectedHrId(e.target.value)}
            className="bg-transparent border-0 text-ink text-sm font-semibold focus:ring-0 focus:outline-none cursor-pointer"
          >
            {props.hrUsers.map(user => (
              <option key={user.id} value={user.id} className="bg-paper text-ink">
                {user.name} ({user.position})
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="glass-toolbar mb-7 flex flex-wrap gap-2 p-2">
        {(
          [
            { key: 'requests', label: '📋 รายการขอลาหยุด', count: requests.filter(r => r.status === 'pending').length },
            { key: 'employees', label: '👥 ทำเนียบพนักงาน', count: employees.length },
            { key: 'calendar', label: '📅 ปฏิทินการลา', count: 0 },
            { key: 'analytics', label: '📊 วิเคราะห์ข้อมูล', count: 0 },
          ] as { key: Tab; label: string; count: number }[]
        ).map((t) => (
          <button
            key={t.key}
            onClick={() => {
              setActiveTab(t.key);
              if (t.key !== 'employees') setSelectedEmployeeId(null);
            }}
            className={`px-4 py-2 text-sm font-bold rounded-lg transition border cursor-pointer ${
              activeTab === t.key
                ? 'bg-accent-strong border-accent/40 text-ink shadow-lg shadow-accent/45'
                : 'bg-paper border-rule text-ink-2 hover:text-ink'
            }`}
          >
            {t.label} {t.count > 0 ? `(${t.count} ${t.key === 'requests' ? 'รออนุมัติ' : 'คน'})` : ''}
          </button>
        ))}
      </div>

      <div className="space-y-8">
        {activeTab === 'requests' && (
          <RequestList
            requests={requests}
            stats={stats}
            deptStats={deptStats}
            maxDeptDays={maxDeptDays}
            statusBadge={statusBadge}
            leaveTypeThai={leaveTypeThai}
            currentMonthLabel={currentMonthLabel}
            onSelectEmployee={(id) => {
              setSelectedEmployeeId(id);
              setActiveTab('employees');
            }}
            onExport={handleExport}
            onRefresh={refresh}
          />
        )}
        {activeTab === 'employees' && (
          <EmployeeDirectory
            employees={employees}
            requests={requests}
            statusBadge={statusBadge}
            leaveTypeThai={leaveTypeThai}
            selectedEmployeeId={selectedEmployeeId}
            onSelectEmployee={setSelectedEmployeeId}
            onRefresh={refresh}
          />
        )}
        {activeTab === 'calendar' && (
          <LeaveCalendar
            requests={requests}
            leaveTypeThai={leaveTypeThai}
            onTooltip={(req, x, y) => setTooltipReq({ req, x, y })}
          />
        )}
        {activeTab === 'analytics' && (
          <Analytics
            stats={stats}
            requests={requests}
            employees={employees}
            deptStats={deptStats}
            maxDeptDays={maxDeptDays}
          />
        )}
      </div>
    </PageLayout>
  );
}

export function HRDashboardClient(props: Props) {
  const initialHrId = props.hrUsers[0]?.id ?? '';
  return (
    <HRProvider initialHrId={initialHrId}>
      <HRDashboardInner {...props} />
    </HRProvider>
  );
}
