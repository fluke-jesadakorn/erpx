'use client';

import { useState } from 'react';
import type { LeaveRequestRow } from '@/hr/server';

const THAI_MONTHS = [
  'มกราคม', 'กุมภาพันธ์', 'มีนาคม', 'เมษายน', 'พฤษภาคม', 'มิถุนายน',
  'กรกฎาคม', 'สิงหาคม', 'กันยายน', 'ตุลาคม', 'พฤศจิกายน', 'ธันวาคม',
];

const DAY_LABELS = ['อา.', 'จ.', 'อ.', 'พ.', 'พฤ.', 'ศ.', 'ส.'];

function leaveChipClass(type: string): string {
  if (type === 'sick') return 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30';
  if (type === 'annual') return 'bg-indigo-500/20 text-indigo-300 border border-indigo-500/30';
  return 'bg-amber-500/20 text-amber-300 border border-amber-500/30';
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
      <div className="flex items-center justify-between bg-slate-900/40 border border-slate-800 px-6 py-4 rounded-2xl">
        <button
          onClick={() => setCalendarDate(new Date(year, month - 1, 1))}
          className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white transition font-bold text-sm cursor-pointer"
        >
          ← ก่อนหน้า
        </button>
        <div className="text-center">
          <h2 className="text-xl font-extrabold bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent">
            {THAI_MONTHS[month]} {year + 543}
          </h2>
          <p className="text-xs text-slate-500 mt-0.5">{year}</p>
        </div>
        <button
          onClick={() => setCalendarDate(new Date(year, month + 1, 1))}
          className="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white transition font-bold text-sm cursor-pointer"
        >
          ถัดไป →
        </button>
      </div>

      <div className="bg-slate-900/30 border border-slate-800 rounded-2xl overflow-hidden">
        <div className="grid grid-cols-7 border-b border-slate-800">
          {DAY_LABELS.map((d, i) => (
            <div
              key={d}
              className={`py-3 text-center text-xs font-bold uppercase tracking-wider ${
                i === 0 ? 'text-rose-400' : i === 6 ? 'text-indigo-400' : 'text-slate-400'
              }`}
            >
              {d}
            </div>
          ))}
        </div>

        <div className="grid grid-cols-7">
          {cells.map((day, idx) => {
            if (day === null) {
              return <div key={`empty-${idx}`} className="min-h-[110px] border-b border-r border-slate-800/60 bg-slate-950/30" />;
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
                className={`min-h-[110px] p-2 border-b border-r border-slate-800/60 flex flex-col gap-1 transition-colors ${
                  isToday
                    ? 'bg-indigo-950/40'
                    : isWeekend
                    ? 'bg-slate-950/50'
                    : 'bg-transparent hover:bg-slate-900/20'
                }`}
              >
                <span
                  className={`text-xs font-bold self-start px-1.5 py-0.5 rounded-md ${
                    isToday
                      ? 'bg-indigo-500 text-white'
                      : isWeekend
                      ? 'text-slate-500'
                      : 'text-slate-400'
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
                    <span className="text-[10px] text-slate-500 font-semibold px-1">+{cellRequests.length - 3} เพิ่มเติม</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="flex flex-wrap gap-4 items-center bg-slate-900/30 border border-slate-800 px-5 py-3 rounded-xl">
        <span className="text-xs text-slate-400 font-bold uppercase tracking-wider">ตำนาน:</span>
        <span className="flex items-center gap-1.5 text-xs text-emerald-300 font-semibold">
          <span className="w-3 h-3 rounded-sm bg-emerald-500/40 border border-emerald-500/50 inline-block" /> ลาป่วย
        </span>
        <span className="flex items-center gap-1.5 text-xs text-indigo-300 font-semibold">
          <span className="w-3 h-3 rounded-sm bg-indigo-500/40 border border-indigo-500/50 inline-block" /> ลาพักร้อน
        </span>
        <span className="flex items-center gap-1.5 text-xs text-amber-300 font-semibold">
          <span className="w-3 h-3 rounded-sm bg-amber-500/40 border border-amber-500/50 inline-block" /> ลากิจ
        </span>
        <span className="ml-auto text-xs text-slate-500 italic">คลิกที่ชิปเพื่อดูรายละเอียด</span>
      </div>
    </div>
  );
}
