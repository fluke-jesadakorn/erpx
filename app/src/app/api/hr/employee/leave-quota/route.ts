import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { updateQuota } from '@folio-lib/hr/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const LINE_PUSH = 'https://api.line.me/v2/bot/message/push';

interface QuotaFlexInput {
  hrName: string;
  employee: { name: string; line_user_id: string | null };
  changes: { label: string; from: number; to: number }[];
  reason: string;
}

function buildQuotaFlex({ hrName, employee, changes, reason }: QuotaFlexInput) {
  const changeContents = changes.flatMap((c) => {
    const delta = c.to - c.from;
    const arrow = delta > 0 ? `▲ +${delta}` : `▼ ${delta}`;
    const arrowColor = delta > 0 ? '#10b981' : '#ef4444';
    return [
      {
        type: 'box',
        layout: 'horizontal',
        margin: 'sm',
        contents: [
          { type: 'text', text: c.label, size: 'sm', flex: 3, color: '#e2e8f0', weight: 'bold' },
          { type: 'text', text: `${c.from} → ${c.to} วัน`, size: 'sm', flex: 3, color: '#94a3b8' },
          { type: 'text', text: arrow, size: 'sm', flex: 2, color: arrowColor, weight: 'bold', align: 'end' },
        ],
      },
    ];
  });

  return {
    type: 'flex',
    altText: `📋 สิทธิ์วันลาของคุณได้รับการปรับปรุงโดย ${hrName}`,
    contents: {
      type: 'bubble',
      size: 'mega',
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
          { type: 'text', text: '📋 อัปเดตสิทธิ์วันลา', weight: 'bold', size: 'xl', color: '#818cf8' },
          { type: 'text', text: `ฝ่ายบุคคลได้ปรับสิทธิ์วันลาของ ${employee.name}`, size: 'sm', color: '#6366f1', margin: 'xs' },
        ],
      },
      body: {
        type: 'box',
        layout: 'vertical',
        backgroundColor: '#0f172a',
        paddingAll: 'lg',
        contents: [
          {
            type: 'box',
            layout: 'horizontal',
            margin: 'none',
            contents: [
              { type: 'text', text: 'ประเภทการลา', size: 'xs', flex: 3, color: '#475569', weight: 'bold' },
              { type: 'text', text: 'เปลี่ยนจาก → เป็น', size: 'xs', flex: 3, color: '#475569', weight: 'bold' },
              { type: 'text', text: 'ผลต่าง', size: 'xs', flex: 2, color: '#475569', weight: 'bold', align: 'end' },
            ],
          },
          { type: 'separator', margin: 'sm', color: '#1e293b' },
          ...changeContents,
          { type: 'separator', margin: 'lg', color: '#1e293b' },
          {
            type: 'box',
            layout: 'horizontal',
            margin: 'md',
            contents: [
              { type: 'text', text: '📝 เหตุผล', size: 'sm', flex: 2, color: '#475569', weight: 'bold' },
              { type: 'text', text: reason, size: 'sm', flex: 5, color: '#94a3b8', wrap: true },
            ],
          },
          {
            type: 'box',
            layout: 'horizontal',
            margin: 'sm',
            contents: [
              { type: 'text', text: '👤 โดย', size: 'sm', flex: 2, color: '#475569', weight: 'bold' },
              { type: 'text', text: hrName, size: 'sm', flex: 5, color: '#e2e8f0', weight: 'bold' },
            ],
          },
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

export async function POST(req: Request) {
  try {
    const body = (await req.json()) as {
      employeeId?: string;
      hrId?: string;
      totalSickLeave?: number;
      totalAnnualLeave?: number;
      totalPersonalLeave?: number;
      reason?: string;
    };
    const { employeeId, hrId, totalSickLeave, totalAnnualLeave, totalPersonalLeave, reason } = body;

    if (!employeeId || !hrId) {
      return NextResponse.json({ success: false, error: 'Missing required fields: employeeId, hrId' }, { status: 400 });
    }
    if (!reason || !reason.trim()) {
      return NextResponse.json({ success: false, error: 'Reason is required (จำเป็นต้องระบุเหตุผล)' }, { status: 400 });
    }
    if (
      totalSickLeave === undefined &&
      totalAnnualLeave === undefined &&
      totalPersonalLeave === undefined
    ) {
      return NextResponse.json({ success: false, error: 'No quota changes specified' }, { status: 400 });
    }
    if (
      (totalSickLeave !== undefined && totalSickLeave < 0) ||
      (totalAnnualLeave !== undefined && totalAnnualLeave < 0) ||
      (totalPersonalLeave !== undefined && totalPersonalLeave < 0)
    ) {
      return NextResponse.json({ success: false, error: 'Leave quota cannot be negative' }, { status: 400 });
    }

    const hrRes = await query<{ name: string }>(
      `SELECT name FROM hr.employees WHERE id = $1 AND role = 'hr'`,
      [hrId],
    );
    if (hrRes.rows.length === 0) {
      return NextResponse.json(
        { success: false, error: 'Unauthorized: HR user not found or insufficient role' },
        { status: 403 },
      );
    }
    const hrName = hrRes.rows[0].name;

    const empRes = await query<{ name: string; line_user_id: string | null }>(
      `SELECT name, line_user_id FROM hr.employees WHERE id = $1`,
      [employeeId],
    );
    if (empRes.rows.length === 0) {
      return NextResponse.json({ success: false, error: 'Employee not found' }, { status: 404 });
    }
    const emp = empRes.rows[0];

    const changes = await updateQuota(
      employeeId,
      {
        totalSickLeave,
        totalAnnualLeave,
        totalPersonalLeave,
      },
      reason,
      hrId,
    );

    const token = process.env.HR_LINE_CHANNEL_ACCESS_TOKEN;
    let notified = false;
    if (token && emp.line_user_id && changes.length > 0) {
      try {
        const flex = buildQuotaFlex({ hrName, employee: { name: emp.name, line_user_id: emp.line_user_id }, changes, reason });
        const r = await fetch(LINE_PUSH, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
          body: JSON.stringify({ to: emp.line_user_id, messages: [flex] }),
        });
        if (!r.ok) {
          const t = await r.text();
          console.error('LINE push error:', t);
        } else {
          notified = true;
          console.log(`LINE Push sent to ${emp.line_user_id}`);
        }
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        console.error('LINE push fetch error:', msg);
      }
    }

    return NextResponse.json({
      success: true,
      changes: changes.map((c) => ({ label: c.label, from: c.from, to: c.to })),
      notified,
    });
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    console.error('leave-quota error:', msg);
    return NextResponse.json({ success: false, error: msg }, { status: 500 });
  }
}
