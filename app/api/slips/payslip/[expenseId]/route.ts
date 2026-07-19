import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { hasPermission, ADMIN_PERM } from '@/perm/auth-client';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface PaymentSlipRow {
  payment_id: string;
  submitter_id: number;
  file_path: string;
  amount: string;
  payment_date: string;
  bank_name: string;
  account_number: string | null;
  payee: string;
  reference: string;
  currency: string;
}

export async function GET(
  req: Request,
  ctx: { params: Promise<{ expenseId: string }> },
) {
  const { expenseId } = await ctx.params;
  const id = Number(expenseId);
  if (!Number.isInteger(id) || id <= 0) {
    return NextResponse.json({ error: 'invalid expenseId' }, { status: 400 });
  }

  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const payment = await query<PaymentSlipRow>(
    `SELECT ep.id::text AS payment_id, e.submitter_id, s.file_path,
            ep.amount::text, ep.payment_date::text, ep.bank_name,
            ep.account_number, ep.payee, ep.reference,
            COALESCE(w.currency, 'THB') AS currency
       FROM expense_payments ep
       JOIN expenses e ON e.id = ep.expense_id
       JOIN waybills w ON w.id = ep.waybill_id
       JOIN slips s ON s.id = ep.slip_id AND s.kind = 'payment_slip'
      WHERE ep.expense_id = $1
      ORDER BY ep.id DESC LIMIT 1`,
    [id],
  );
  const row = payment.rows[0];
  if (!row) return NextResponse.json({ error: 'paid slip not found' }, { status: 404 });

  const session = {
    user: { id: actor.id, name: actor.fullname, role: actor.role_id ?? 'unconfigured' },
    permissions: actor.permissions,
  };
  const allowed = actor.permissions.includes(ADMIN_PERM)
    || row.submitter_id === actor.id
    || hasPermission(session, 'finance:expense:settle::allow')
    || hasPermission(session, 'finance:expense:view_all::allow');
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const slipUrl = `/api/slips/file?key=${encodeURIComponent(row.file_path)}`;
  if (new URL(req.url).searchParams.get('format') === 'json') {
    return NextResponse.json({
      paymentId: Number(row.payment_id),
      expenseId: id,
      amount: Number(row.amount),
      currency: row.currency.trim(),
      paymentDate: row.payment_date,
      bankName: row.bank_name,
      accountNumber: row.account_number,
      payee: row.payee,
      reference: row.reference,
      simulated: true,
      slipUrl,
    });
  }
  return NextResponse.redirect(new URL(slipUrl, req.url), 302);
}
