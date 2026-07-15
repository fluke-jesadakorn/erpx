import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { loadActor } from '@folio-lib/server/guard';
import { canManageResource, hasPermission, ADMIN_PERM } from '@folio-lib/perm/auth-client';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface ExpenseRow {
  id: number;
  vendor_name: string;
  total_amount: number;
  vat_amount: number;
  subtotal: number;
  transaction_date: string;
  payment_method: string | null;
  status: string;
  submitter_id: number;
  submitter_name: string;
  submitter_dept: string | null;
  submitter_dept_th: string | null;
  submitter_dept_de: string | null;
  po_number: string | null;
  settled_at: string | null;
  settled_by_name: string | null;
}

async function fetchExpense(expenseId: number): Promise<ExpenseRow | null> {
  const r = await query<ExpenseRow>(
    `SELECT e.id, e.vendor_name, e.total_amount, e.vat_amount, e.subtotal,
            e.transaction_date, e.payment_method, e.status,
            e.submitter_id,
            u.fullname AS submitter_name,
            (SELECT split_part(up.permission_id, ':', 3) FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS submitter_dept,
            (SELECT split_part(up.permission_id, ':', 3) FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS submitter_dept_th,
            (SELECT split_part(up.permission_id, ':', 3) FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS submitter_dept_de,
            (SELECT po.po_number FROM purchase_orders po
              WHERE po.vendor_name = e.vendor_name
                AND po.total_amount = e.total_amount
                AND po.status = 'settled'
              ORDER BY po.id DESC LIMIT 1) AS po_number,
            e.updated_at AS settled_at,
            (SELECT u2.fullname FROM purchase_orders po
              JOIN users u2 ON u2.id = po.settled_by
              WHERE po.vendor_name = e.vendor_name
                AND po.total_amount = e.total_amount
                AND po.status = 'settled'
              ORDER BY po.id DESC LIMIT 1) AS settled_by_name
       FROM expenses e
       JOIN users u ON u.id = e.submitter_id
      WHERE e.id = $1`,
    [expenseId],
  );
  return r.rows[0] ?? null;
}

const fmt = (n: number) =>
  n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

function renderSvg(exp: ExpenseRow): string {
  const W = 880;
  const H = 520;
  const txDate = exp.transaction_date
    ? new Date(exp.transaction_date).toLocaleDateString('en-GB', { day: '2-digit', month: 'short', year: 'numeric' })
    : '—';
  const settledAt = exp.settled_at
    ? new Date(exp.settled_at).toLocaleString('en-GB', { day: '2-digit', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' })
    : '—';
  const method = (exp.payment_method || 'cash').toUpperCase();
  const methodGlyph = method === 'CASH' ? '💵' : method === 'CREDIT_CARD' ? '💳' : '🏦';

  const bankName = process.env.PAYSLIP_BANK_NAME || 'KASIKORNBANK';
  const bankBranch = process.env.PAYSLIP_BANK_BRANCH || 'Siam Square';
  const bankAccount = process.env.PAYSLIP_BANK_ACCOUNT || '123-4-56789-0';
  const bankSwift = process.env.PAYSLIP_BANK_SWIFT || 'KASITHBK';
  const bankRouting = process.env.PAYSLIP_BANK_ROUTING || '0004';

  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" font-family="ui-sans-serif, system-ui, 'Segoe UI', sans-serif">
  <defs>
    <linearGradient id="hdr" x1="0" x2="1" y1="0" y2="0">
      <stop offset="0" stop-color="#0f172a"/>
      <stop offset="1" stop-color="#1e293b"/>
    </linearGradient>
    <pattern id="grid" width="32" height="32" patternUnits="userSpaceOnUse">
      <path d="M32 0H0V32" fill="none" stroke="#e2e8f0" stroke-width="0.5" opacity="0.6"/>
    </pattern>
  </defs>

  <rect width="${W}" height="${H}" fill="#f8fafc"/>
  <rect width="${W}" height="${H}" fill="url(#grid)"/>

  <rect x="0" y="0" width="${W}" height="80" fill="url(#hdr)"/>
  <text x="40" y="50" font-size="22" font-weight="800" fill="#f8fafc" letter-spacing="2">WORLD ERP CORPORATE CO., LTD.</text>
  <text x="40" y="70" font-size="11" fill="#94a3b8">Payment Slip · Internal Disbursement Document</text>

  <g transform="translate(680, 18)">
    <rect width="170" height="46" rx="6" fill="#10b981"/>
    <text x="${170 / 2}" y="29" font-size="18" font-weight="800" fill="#022c22" text-anchor="middle" letter-spacing="2">PAID</text>
    <text x="${170 / 2}" y="42" font-size="9" fill="#022c22" text-anchor="middle">SETTLED</text>
  </g>

  <g transform="translate(40, 110)">
    <text font-size="10" fill="#64748b" letter-spacing="1">DOC NO.</text>
    <text y="22" font-size="20" font-weight="700" fill="#0f172a">EXP-${exp.id}</text>
  </g>
  <g transform="translate(220, 110)">
    <text font-size="10" fill="#64748b" letter-spacing="1">PO NUMBER</text>
    <text y="22" font-size="20" font-weight="700" fill="#0f172a">${exp.po_number ?? '—'}</text>
  </g>
  <g transform="translate(450, 110)">
    <text font-size="10" fill="#64748b" letter-spacing="1">TRANSACTION DATE</text>
    <text y="22" font-size="16" font-weight="600" fill="#0f172a">${txDate}</text>
  </g>
  <g transform="translate(700, 110)">
    <text font-size="10" fill="#64748b" letter-spacing="1">SETTLED</text>
    <text y="22" font-size="13" font-weight="600" fill="#0f172a">${settledAt}</text>
  </g>

  <line x1="40" y1="160" x2="${W - 40}" y2="160" stroke="#cbd5e1" stroke-dasharray="4 4"/>

  <g transform="translate(40, 180)">
    <text font-size="10" fill="#64748b" letter-spacing="1">BENEFICIARY (EMPLOYEE)</text>
    <text y="20" font-size="14" font-weight="600" fill="#0f172a">${escape(exp.submitter_name)}</text>
    <text y="38" font-size="11" fill="#475569">${escape(exp.submitter_dept ?? '—')}</text>
  </g>

  <g transform="translate(40, 250)">
    <text font-size="10" fill="#64748b" letter-spacing="1">VENDOR / MERCHANT</text>
    <text y="20" font-size="14" font-weight="600" fill="#0f172a">${escape(exp.vendor_name || 'Unknown Vendor')}</text>
  </g>

  <g transform="translate(40, 310)">
    <rect width="800" height="80" rx="8" fill="#ffffff" stroke="#e2e8f0"/>
    <text x="20" y="28" font-size="10" fill="#64748b" letter-spacing="1">SUBTOTAL</text>
    <text x="20" y="55" font-size="16" font-weight="600" fill="#0f172a" font-family="ui-monospace, 'SF Mono', monospace">${fmt(Number(exp.subtotal) || 0)} THB</text>
    <text x="280" y="28" font-size="10" fill="#64748b" letter-spacing="1">VAT</text>
    <text x="280" y="55" font-size="16" font-weight="600" fill="#0f172a" font-family="ui-monospace, 'SF Mono', monospace">${fmt(Number(exp.vat_amount) || 0)} THB</text>
    <text x="540" y="28" font-size="10" fill="#64748b" letter-spacing="1">TOTAL</text>
    <text x="540" y="60" font-size="26" font-weight="800" fill="#047857" font-family="ui-monospace, 'SF Mono', monospace">${fmt(Number(exp.total_amount) || 0)} THB</text>
  </g>

  <g transform="translate(40, 410)">
    <rect width="400" height="90" rx="8" fill="#fff7ed" stroke="#fdba74"/>
    <text x="20" y="22" font-size="10" fill="#9a3412" letter-spacing="1">PAYER BANK</text>
    <text x="20" y="44" font-size="13" font-weight="700" fill="#7c2d12">${escape(bankName)}</text>
    <text x="20" y="62" font-size="10" fill="#7c2d12">Branch: ${escape(bankBranch)}</text>
    <text x="20" y="78" font-size="10" fill="#7c2d12">Account: ${escape(bankAccount)} · SWIFT: ${escape(bankSwift)} · Routing: ${escape(bankRouting)}</text>
  </g>

  <g transform="translate(450, 410)">
    <rect width="390" height="90" rx="8" fill="#ecfeff" stroke="#67e8f9"/>
    <text x="20" y="22" font-size="10" fill="#155e75" letter-spacing="1">PAYMENT METHOD</text>
    <text x="20" y="50" font-size="20" font-weight="800" fill="#0e7490">${methodGlyph} ${method}</text>
    <text x="20" y="72" font-size="10" fill="#155e75">Internal routing reference only</text>
  </g>
</svg>`;
}

function escape(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

export async function GET(
  req: Request,
  ctx: { params: Promise<{ expenseId: string }> },
) {
  const { expenseId } = await ctx.params;
  const id = Number(expenseId);
  if (!Number.isFinite(id) || id <= 0) {
    return NextResponse.json({ error: 'invalid expenseId' }, { status: 400 });
  }

  const actor = await loadActor();
  if (!actor) {
    return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  }
  const session = {
    user: { id: actor.id, name: actor.fullname, role: actor.role_id ?? 'officer::5' },
    permissions: actor.permissions,
  };
  const subRes = await query<{ submitter_id: number | null }>(`SELECT submitter_id FROM expenses WHERE id = $1`, [id]);
  const submitterId = subRes.rows[0]?.submitter_id ?? null;
  const allow =
    actor.permissions.includes(ADMIN_PERM)
    || (submitterId !== null && submitterId === actor.id)
    || hasPermission(session, 'finance:expense:settle::allow')
    || hasPermission(session, 'finance:expense:view_all::allow');
  if (!allow) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }

  const exp = await fetchExpense(id);
  if (!exp) return NextResponse.json({ error: 'not found' }, { status: 404 });
  if (exp.status !== 'paid') {
    return NextResponse.json({ error: `expense is '${exp.status}', only 'paid' generates a slip` }, { status: 409 });
  }

  const url = new URL(req.url);
  const format = url.searchParams.get('format');

  const svg = renderSvg(exp);

  if (format === 'json') {
    return NextResponse.json({
      expenseId: exp.id,
      poNumber: exp.po_number,
      vendor: exp.vendor_name,
      total: Number(exp.total_amount),
      currency: 'THB',
      method: exp.payment_method,
      svgUrl: `/api/slips/payslip/${exp.id}`,
    });
  }

  return new Response(svg, {
    status: 200,
    headers: {
      'Content-Type': 'image/svg+xml; charset=utf-8',
      'Cache-Control': 'private, max-age=60',
      'X-Expense-Id': String(exp.id),
    },
  });
}
