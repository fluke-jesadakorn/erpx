import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const LINE_PUSH = 'https://api.line.me/v2/bot/message/push';

function leaveTypeLabel(type: string): string {
  if (type === 'sick') return '🤒 ลาป่วย';
  if (type === 'annual') return '✈️ ลาพักร้อน';
  if (type === 'personal') return '💼 ลากิจ';
  return type;
}

export async function POST(req: Request) {
  try {
    const body = (await req.json()) as { employeeId?: string; employeeName?: string; department?: string };
    const { employeeId, employeeName, department } = body;

    if (!employeeId || !department) {
      return NextResponse.json({ success: false, error: 'Missing required fields' }, { status: 400 });
    }

    const token = process.env.HR_LINE_CHANNEL_ACCESS_TOKEN;
    if (!token) {
      return NextResponse.json({ success: false, error: 'No LINE token available' }, { status: 500 });
    }

    const leaveRes = await query<{
      leave_type: string;
      start_date: string;
      end_date: string;
      days: string | number;
      reason: string;
      created_at: string;
    }>(
      `SELECT lr.leave_type, lr.start_date::text, lr.end_date::text, lr.days, lr.reason, lr.created_at::text
         FROM hr.leave_requests lr
        WHERE lr.employee_id = $1 AND lr.status = 'pending'
        ORDER BY lr.created_at DESC LIMIT 1`,
      [employeeId],
    );
    const leave = leaveRes.rows[0];

    const managerRes = await query<{ line_user_id: string; name: string }>(
      `SELECT line_user_id, name FROM hr.employees
        WHERE ((role = 'manager' AND department = $1) OR role = 'hr')
          AND line_user_id IS NOT NULL
        ORDER BY (role = 'manager' AND department = $1) DESC
        LIMIT 1`,
      [department],
    );
    if (managerRes.rows.length === 0) {
      return NextResponse.json({ success: false, error: 'No manager/HR found to notify' });
    }
    const manager = managerRes.rows[0];

    const flexMessage = {
      type: 'flex',
      altText: `🔔 คำขอลาใหม่จาก ${employeeName}`,
      contents: {
        type: 'bubble',
        styles: {
          header: { backgroundColor: '#1e1b4b' },
          body: { backgroundColor: '#0f172a' },
          footer: { backgroundColor: '#0f172a' },
        },
        header: {
          type: 'box',
          layout: 'vertical',
          backgroundColor: '#1e1b4b',
          paddingAll: 'lg',
          contents: [
            { type: 'text', text: '🔔 คำขอลาใหม่', weight: 'bold', size: 'xl', color: '#818cf8' },
            { type: 'text', text: 'รออนุมัติจากคุณ', size: 'sm', color: '#6366f1', margin: 'xs' },
          ],
        },
        body: {
          type: 'box',
          layout: 'vertical',
          backgroundColor: '#0f172a',
          paddingAll: 'lg',
          contents: [
            { type: 'text', text: `👤 ${employeeName ?? ''}`, weight: 'bold', size: 'md', color: '#e2e8f0' },
            { type: 'text', text: `แผนก: ${department}`, size: 'sm', color: '#64748b', margin: 'xs' },
            { type: 'separator', margin: 'md', color: '#1e293b' },
            ...(leave
              ? [
                  {
                    type: 'box',
                    layout: 'horizontal',
                    margin: 'md',
                    contents: [
                      { type: 'text', text: 'ประเภท:', size: 'sm', color: '#475569', flex: 2 },
                      { type: 'text', text: leaveTypeLabel(leave.leave_type), size: 'sm', weight: 'bold', flex: 3, color: '#e2e8f0' },
                    ],
                  },
                  {
                    type: 'box',
                    layout: 'horizontal',
                    margin: 'sm',
                    contents: [
                      { type: 'text', text: 'ระยะเวลา:', size: 'sm', color: '#475569', flex: 2 },
                      { type: 'text', text: `${leave.start_date} – ${leave.end_date} (${leave.days} วัน)`, size: 'sm', weight: 'bold', flex: 3, color: '#e2e8f0' },
                    ],
                  },
                  {
                    type: 'box',
                    layout: 'horizontal',
                    margin: 'sm',
                    contents: [
                      { type: 'text', text: 'เหตุผล:', size: 'sm', color: '#475569', flex: 2 },
                      { type: 'text', text: leave.reason, size: 'sm', flex: 3, wrap: true, color: '#94a3b8' },
                    ],
                  },
                ]
              : [
                  { type: 'text', text: 'ไม่พบรายละเอียดใบลา', size: 'sm', color: '#475569', margin: 'md' },
                ]),
          ],
        },
        footer: {
          type: 'box',
          layout: 'vertical',
          backgroundColor: '#0f172a',
          contents: [
            {
              type: 'button',
              action: { type: 'uri', label: '📋 ดูและอนุมัติที่ Web Admin', uri: 'http://localhost:3003/hr' },
              style: 'primary',
              color: '#4f46e5',
            },
            { type: 'text', text: '- HR Leave Portal', size: 'xs', color: '#334155', align: 'end', margin: 'sm' },
          ],
        },
      },
    };

    const pushRes = await fetch(LINE_PUSH, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
      body: JSON.stringify({ to: manager.line_user_id, messages: [flexMessage] }),
    });

    if (!pushRes.ok) {
      const err = await pushRes.text();
      console.error('LINE push failed:', err);
      return NextResponse.json({ success: false, error: 'LINE push failed', detail: err }, { status: 500 });
    }

    return NextResponse.json({ success: true, notifiedManager: manager.name });
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    console.error('notify-manager error:', msg);
    return NextResponse.json({ success: false, error: msg }, { status: 500 });
  }
}
