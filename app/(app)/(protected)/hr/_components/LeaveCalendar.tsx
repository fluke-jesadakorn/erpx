'use client';

import { useState } from 'react';
import type { LeaveRequestRow } from '@/hr/server';

const THAI_MONTHS = [
  'มกราคม', 'กุมภาพันธ์', 'มีนาคม', 'เมษายน', 'พฤษภาคม', 'มิถุนายน',
  'กรกฎาคม', 'สิงหาคม', 'กันยายน', 'ตุลาคม', 'พฤศจิกายน', 'ธันวาคม',
];

const DAY_LABELS = ['อา.', 'จ.', 'อ.', 'พ.', 'พฤ.', 'ศ.', 'ส.'];

function leaveChipClass(type: string): string {
  if (type === 'sick') return 'bg-positive text-positive border border-positive/40';
  if (type === 'annual') return 'bg-accent text-accent border border-accent/40';
  return 'bg-caution text-caution border border-caution/40';
}

function toYMD(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function getDaysInMonth(year: number, month: number) {
  return new Date(year, month + 1, 0).getDate();
}
function getFirstDayOfMonth(year: number, month: number) {
  return new Date(year, month, 1).getDay();
}

interface Props {
  requests: LeaveRequestRow[];
  leaveTypeThai: (type: string) => string;
  onTooltip: (req: LeaveRequestRow, x: number, y: number) => void;
}

export function LeaveCalendar({ requests, leaveTypeThai, onTooltip }: Props) {
  const [calendarDate, setCalendarDate] = useState(new Date());
  const year = calendarDate.getFullYear();
  const month = calendarDate.getMonth();
  const daysInMonth = getDaysInMonth(year, month);
  const firstDay = getFirstDayOfMonth(year, month);
  const cells: (number | null)[] = [
    ...Array(firstDay).fill(null),
    ...Array.from({ length: daysInMonth }, (_, i) => i + 1),
  ];
  while (cells.length % 7 !== 0) cells.push(null);

  const getRequestsForDate = (cellDate: string) =>
    requests.filter(
      (req) => req.status === 'approved' && req.start_date <= cellDate && req.end_date >= cellDate,
    );

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between bg-paper-2/40 border border-rule px-6 py-4 rounded-md">
        <button
          onClick={() => setCalendarDate(new Date(year, month - 1, 1))}
          className="px-4 py-2 rounded-md bg-paper-2 hover:bg-paper-2 text-ink-2 hover:text-ink transition font-bold text-sm cursor-pointer"
        >
          ← ก่อนหน้า
        </button>
        <div className="text-center">
          <h2 className="text-xl font-extrabold bg-gradient-to-r from-accent to-accent bg-clip-text text-transparent">
            {THAI_MONTHS[month]} {year + 543}
          </h2>
          <p className="text-xs text-mute mt-0.5">{year}</p>
        </div>
        <button
          onClick={() => setCalendarDate(new Date(year, month + 1, 1))}
          className="px-4 py-2 rounded-md bg-paper-2 hover:bg-paper-2 text-ink-2 hover:text-ink transition font-bold text-sm cursor-pointer"
        >
          ถัดไป →
        </button>
      </div>

      <div className="bg-paper-2/30 border border-rule rounded-md overflow-hidden">
        <div className="grid grid-cols-7 border-b border-rule">
          {DAY_LABELS.map((d, i) => (
            <div
              key={d}
              className={`py-3 text-center text-xs font-bold uppercase tracking-wider ${
                i === 0 ? 'text-critical' : i === 6 ? 'text-accent' : 'text-ink-2'
              }`}
            >
              {d}
            </div>
          ))}
        </div>

        <div className="grid grid-cols-7">
          {cells.map((day, idx) => {
            if (day === null) {
              return <div key={`empty-${idx}`} className="min-h-[110px] border-b border-r border-rule/60 bg-paper-2/30" />;
            }
            const cellDate = `${year}-${String(month + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
            const todayStr = toYMD(new Date());
            const isToday = cellDate === todayStr;
            const cellRequests = getRequestsForDate(cellDate);
            const dayOfWeek = (firstDay + day - 1) % 7;
            const isWeekend = dayOfWeek === 0 || dayOfWeek === 6;

            return (
              <div
                key={cellDate}
                className={`min-h-[110px] p-2 border-b border-r border-rule/60 flex flex-col gap-1 transition-colors ${
                  isToday
                    ? 'bg-accent-strong'
                    : isWeekend
                    ? 'bg-paper-2/50'
                    : 'bg-transparent hover:bg-paper-2/20'
                }`}
              >
                <span
                  className={`text-xs font-bold self-start px-1.5 py-0.5 rounded-md ${
                    isToday
                      ? 'bg-accent text-ink'
                      : isWeekend
                      ? 'text-mute'
                      : 'text-ink-2'
                  }`}
                >
                  {day}
                </span>
                <div className="flex flex-col gap-0.5 flex-1">
                  {cellRequests.slice(0, 3).map((req) => (
                    <button
                      key={req.id}
                      onClick={(e) => {
                        e.stopPropagation();
                        onTooltip(req, e.clientX, e.clientY);
                      }}
                      title={`${req.employee_name} - ${leaveTypeThai(req.leave_type)}`}
                      className={`text-[10px] px-1.5 py-0.5 rounded-md truncate text-left font-semibold cursor-pointer hover:opacity-80 transition-opacity ${leaveChipClass(req.leave_type)}`}
                    >
                      {req.employee_name.split(' ')[0]}
                    </button>
                  ))}
                  {cellRequests.length > 3 && (
                    <span className="text-[10px] text-mute font-semibold px-1">+{cellRequests.length - 3} เพิ่มเติม</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="flex flex-wrap gap-4 items-center bg-paper-2/30 border border-rule px-5 py-3 rounded-md">
        <span className="text-xs text-ink-2 font-bold uppercase tracking-wider">ตำนาน:</span>
        <span className="flex items-center gap-1.5 text-xs text-positive font-semibold">
          <span className="w-3 h-3 rounded-sm bg-positive border border-positive/40 inline-block" /> ลาป่วย
        </span>
        <span className="flex items-center gap-1.5 text-xs text-accent font-semibold">
          <span className="w-3 h-3 rounded-sm bg-accent border border-accent/40 inline-block" /> ลาพักร้อน
        </span>
        <span className="flex items-center gap-1.5 text-xs text-caution font-semibold">
          <span className="w-3 h-3 rounded-sm bg-caution border border-caution/40 inline-block" /> ลากิจ
        </span>
        <span className="ml-auto text-xs text-mute italic">คลิกที่ชิปเพื่อดูรายละเอียด</span>
      </div>
    </div>
  );
}
