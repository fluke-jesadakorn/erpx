import { NextResponse } from 'next/server';
import { approveLeave, rejectLeave } from '@folio-lib/hr/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const LINE_PUSH = 'https://api.line.me/v2/bot/message/push';

function leaveTypeThai(type: string): string {
  if (type === 'sick') return '🤒 ลาป่วย';
  if (type === 'annual') return '✈️ ลาพักร้อน';
  if (type === 'personal') return '💼 ลากิจ';
  return type;
}

function buildFlexApproveReject({
  statusText,
  statusSub,
  statusBg,
  statusAccent,
  leaveType,
  startDate,
  endDate,
  days,
  hrName,
  rejectReason,
}: {
  statusText: string;
  statusSub: string;
  statusBg: string;
  statusAccent: string;
  leaveType: string;
  startDate: string;
  endDate: string;
  days: number;
  hrName: string;
  rejectReason?: string;
}) {
  const rows: object[] = [
    {
      type: 'box',
      layout: 'horizontal',
      contents: [
        { type: 'text', text: 'ประเภท', size: 'sm', color: '#64748b', flex: 2 },
        { type: 'text', text: leaveType, size: 'sm', color: '#e2e8f0', weight: 'bold', flex: 3 },
      ],
    },
    {
      type: 'box',
      layout: 'horizontal',
      margin: 'sm',
      contents: [
        { type: 'text', text: 'ระยะเวลา', size: 'sm', color: '#64748b', flex: 2 },
        { type: 'text', text: `${startDate} – ${endDate} (${days} วัน)`, size: 'sm', color: '#e2e8f0', flex: 3 },
      ],
    },
    {
      type: 'box',
      layout: 'horizontal',
      margin: 'sm',
      contents: [
        { type: 'text', text: 'ผู้อนุมัติ', size: 'sm', color: '#64748b', flex: 2 },
        { type: 'text', text: hrName, size: 'sm', color: '#e2e8f0', weight: 'bold', flex: 3 },
      ],
    },
  ];
  if (rejectReason) {
    rows.push({ type: 'separator', margin: 'md' });
    rows.push({
      type: 'box',
      layout: 'horizontal',
      margin: 'md',
      contents: [
        { type: 'text', text: 'เหตุผล', size: 'sm', color: '#64748b', flex: 2 },
        { type: 'text', text: rejectReason, size: 'sm', color: '#fda4af', wrap: true, flex: 3 },
      ],
    });
  }

  return {
    type: 'flex',
    altText: `${statusText} ${leaveType} ${days} วัน`,
    contents: {
      type: 'bubble',
      styles: { body: { backgroundColor: '#0f172a' }, footer: { backgroundColor: '#0f172a' } },
      header: {
        type: 'box',
        layout: 'vertical',
        backgroundColor: statusBg,
        paddingAll: 'lg',
        contents: [
          { type: 'text', text: statusText, weight: 'bold', size: 'xl', color: statusAccent },
          { type: 'text', text: statusSub, size: 'sm', color: '#94a3b8', margin: 'xs' },
        ],
      },
      body: {
        type: 'box',
        layout: 'vertical',
        backgroundColor: '#0f172a',
        paddingAll: 'lg',
        contents: [
          { type: 'separator', color: '#1e293b' },
          { type: 'box', layout: 'vertical', margin: 'lg', spacing: 'xs', contents: rows },
        ],
      },
      footer: {
        type: 'box',
        layout: 'vertical',
        backgroundColor: '#0f172a',
        contents: [{ type: 'text', text: '- HR Leave Portal', size: 'xs', color: '#334155', align: 'end' }],
      },
    },
  };
}

async function pushLine(lineUserId: string, message: object): Promise<void> {
  const token = process.env.HR_LINE_CHANNEL_ACCESS_TOKEN;
  if (!token) {
    console.warn('HR_LINE_CHANNEL_ACCESS_TOKEN not configured in process.env');
    return;
  }
  try {
    const r = await fetch(LINE_PUSH, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ to: lineUserId, messages: [message] }),
    });
    if (!r.ok) {
      const t = await r.text();
      console.error('LINE Push API error:', t);
    } else {
      console.log(`LINE Push sent to ${lineUserId}`);
    }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error('Fetch error LINE Push:', msg);
  }
}

export async function POST(req: Request) {
  try {
    const body = await req.json();
    const { requestId, action, hrId, rejectReason } = body as {
      requestId?: string;
      action?: 'approve' | 'reject';
      hrId?: string;
      rejectReason?: string;
    };

    if (!requestId || !action || !hrId) {
      return NextResponse.json(
        { success: false, error: 'Missing required parameters: requestId, action, hrId' },
        { status: 400 },
      );
    }
    if (action !== 'approve' && action !== 'reject') {
      return NextResponse.json(
        { success: false, error: 'Invalid action. Must be "approve" or "reject"' },
        { status: 400 },
      );
    }
    if (action === 'reject' && (!rejectReason || !rejectReason.trim())) {
      return NextResponse.json(
        { success: false, error: 'Rejection reason is required' },
        { status: 400 },
      );
    }

    const decision =
      action === 'approve'
        ? await approveLeave(requestId, hrId)
        : await rejectLeave(requestId, hrId, rejectReason || '');

    if (!decision) {
      return NextResponse.json(
        { success: false, error: 'Unknown leave request' },
        { status: 404 },
      );
    }

    if (decision.line_user_id) {
      const lt = leaveTypeThai(decision.leave_type);
      const isApproved = action === 'approve';
      const message = buildFlexApproveReject({
        statusText: isApproved ? '✅ อนุมัติแล้ว' : '❌ ปฏิเสธแล้ว',
        statusSub: isApproved ? 'คำขอลาของคุณได้รับการอนุมัติ' : 'คำขอลาของคุณถูกปฏิเสธ',
        statusBg: isApproved ? '#064e3b' : '#4c0519',
        statusAccent: isApproved ? '#10b981' : '#f43f5e',
        leaveType: lt,
        startDate: decision.start_date,
        endDate: decision.end_date,
        days: decision.days,
        hrName: decision.hr_name,
        rejectReason: action === 'reject' ? rejectReason : undefined,
      });
      await pushLine(decision.line_user_id, message);
    }

    return NextResponse.json({ success: true, status: decision.status });
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    console.error('Error processing leave decision:', msg);
    return NextResponse.json({ success: false, error: msg }, { status: 500 });
  }
}
