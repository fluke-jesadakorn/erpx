import React from 'react';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import type {
  ExpenseFullPicture,
  WaybillSlip,
} from '@folio-lib/waybill/queries';
import { fmtSize, statusPill } from './ui';
import { SlipThumbZoom } from './SlipThumbZoom';
import { formatDateServer, formatMoneyServer } from '@/components/i18n/formattersServer';
import { Bilingual } from '@/components/i18n/Bilingual';

interface Props {
  data: ExpenseFullPicture;
  waybillId: string;
  currentStage: string;
  locale?: SecondaryLocale;
}

async function presignedOrHash(slip: WaybillSlip): Promise<string> {
  try {
    return `/api/slips/file?key=${encodeURIComponent(slip.file_path)}`;
  } catch {
    return '#';
  }
}

function slipUrl(slip: WaybillSlip): string {
  return `/api/slips/file?key=${encodeURIComponent(slip.file_path)}`;
}

function paymentLabel(method: string | null): string {
  if (!method) return '—';
  const m: Record<string, string> = {
    cash: 'Cash',
    credit_card: 'Credit card',
    transfer: 'Transfer',
  };
  return m[method] ?? method;
}

function SlipRow({
  emoji,
  label,
  slip,
  href,
  locale,
  bankFields,
}: {
  emoji: string;
  label: React.ReactNode;
  slip: WaybillSlip | null;
  href: string | null;
  locale: SecondaryLocale;
  bankFields?: {
    bankName: string | null;
    accountNumber: string | null;
    accountName: string | null;
    bankBranch: string | null;
  } | null;
}) {
  const showThumb = !!slip && !!href && href !== '#' && /^image\//.test(slip.mime_type);
  return (
    <div className="py-2">
      <div className="flex items-center gap-2">
        <span aria-hidden className="text-lg leading-none">
          {emoji}
        </span>
        <span className="text-sm font-bold text-white">{label}</span>
        {slip ? (
          <span className="ml-auto rounded-full border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 text-sm font-mono uppercase tracking-widest text-cyan-200">
            ✓ {slip.status}
          </span>
        ) : (
          <span className="glass-panel ml-auto rounded-full border px-2 py-0.5 text-sm font-mono uppercase tracking-widest text-slate-500">
            <Bilingual en="none" th="ไม่มี" de="keine" locale={locale} />
          </span>
        )}
      </div>
      {slip && href ? (
        <div className="mt-1.5 flex items-start gap-3 text-xs">
          {showThumb && (
            <SlipThumbZoom
              href={href}
              alt={typeof label === 'string' ? label : 'slip'}
              title={slip.file_path.split('/').slice(-1)[0]}
              subtitle={`${fmtSize(slip.file_size)} · ${slip.mime_type}`}
              className="h-16 w-12"
            />
          )}
          <div className="min-w-0 flex-1 space-y-1">
            <a
              href={href}
              className="block break-all font-mono text-sm font-semibold text-cyan-300 underline-offset-2 hover:underline"
            >
              {slip.file_path.split('/').slice(-1)[0]}
            </a>
            <div className="text-xs font-mono text-slate-500">
              {fmtSize(slip.file_size)} · {slip.mime_type}
            </div>
            {bankFields && (bankFields.bankName || bankFields.accountNumber) && (
              <div className="text-xs font-mono text-slate-300">
                {bankFields.bankName && (
                  <span className="mr-3">
                    🏦 <span className="text-slate-100">{bankFields.bankName}</span>
                  </span>
                )}
                {bankFields.accountNumber && (
                  <span className="mr-3">
                    #<span className="text-cyan-300 font-bold">{bankFields.accountNumber}</span>
                  </span>
                )}
                {bankFields.accountName && (
                  <span>
                    👤 <span className="text-slate-100">{bankFields.accountName}</span>
                  </span>
                )}
              </div>
            )}
          </div>
        </div>
      ) : (
        <p className="mt-1.5 text-xs italic text-slate-500">
          <Bilingual en="not attached" th="ยังไม่ได้แนบ" de="nicht angehängt" locale={locale} />
        </p>
      )}
    </div>
  );
}

function Field({
  label,
  value,
  mono,
  accent,
  full,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
  accent?: string;
  full?: boolean;
}) {
  return (
    <div className={full ? 'col-span-2' : ''}>
      <dt className="text-sm font-mono uppercase tracking-widest text-slate-500">{label}</dt>
      <dd
        className={
          'mt-0.5 text-sm text-slate-100' +
          (mono ? ' font-mono' : '') +
          (accent ? ' ' + accent : '')
        }
      >
        {value}
      </dd>
    </div>
  );
}

function ContextShell({
  tone,
  badge,
  hint,
  children,
}: {
  tone: 'cyan' | 'amber' | 'indigo' | 'emerald' | 'rose' | 'slate';
  badge: React.ReactNode;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  const toneCls: Record<typeof tone, string> = {
    cyan: 'border-cyan-500/30 bg-cyan-950/15',
    amber: 'border-amber-500/30 bg-amber-950/15',
    indigo: 'border-indigo-500/30 bg-indigo-950/15',
    emerald: 'border-emerald-500/30 bg-emerald-950/15',
    rose: 'border-rose-500/30 bg-rose-950/15',
    slate: 'glass-panel',
  };
  const headerCls: Record<typeof tone, string> = {
    cyan: 'text-cyan-200',
    amber: 'text-amber-200',
    indigo: 'text-indigo-200',
    emerald: 'text-emerald-200',
    rose: 'text-rose-200',
    slate: 'text-slate-200',
  };
  return (
    <section className={'space-y-3 rounded-2xl border p-4 ' + toneCls[tone]}>
      <header className="flex flex-wrap items-center gap-2 text-sm font-mono uppercase tracking-widest">
        <span className={headerCls[tone]}>{badge}</span>
      </header>
      {hint && <p className="text-sm italic text-slate-400">{hint}</p>}
      {children}
    </section>
  );
}

function StepContextBlock({
  pipKey,
  picture,
  hasGlConfirmed,
  locale,
}: {
  pipKey: string;
  picture: ExpenseFullPicture | null;
  hasGlConfirmed: boolean;
  locale: SecondaryLocale;
}) {
  if (!picture) return null;
  const { slips } = picture;
  const receipt = slips.find((s) => (s as any).kind === 'receipt') ?? null;
  const bookBank = slips.find((s) => (s as any).kind === 'book_bank' || (s as any).kind === 'book-bank') ?? null;
  const receiptHref = receipt ? slipUrl(receipt) : null;
  const bookHref = bookBank ? slipUrl(bookBank) : null;

  switch (pipKey) {
    case 'draft':
    case 'submission':
      return <SubmissionContext picture={picture} receipt={receipt} receiptHref={receiptHref} bookBank={bookBank} bookHref={bookHref} locale={locale} />;
    case 'dept_verification':
    case 'dept_authorization':
      return <DeptContext picture={picture} receipt={receipt} receiptHref={receiptHref} locale={locale} />;
    case 'accounting_verification':
    case 'accounting_supervision':
    case 'accounting_authorization':
      return <AccountingContext picture={picture} locale={locale} />;
    case 'final_authorization':
    case 'disbursement_authorization':
    case 'cfo_authorization':
    case 'ceo_authorization':
      return <FinalContext picture={picture} bookBank={bookBank} bookHref={bookHref} locale={locale} />;
    case 'awaiting_disbursement':
      return <AwaitingDisbursementContext picture={picture} bookBank={bookBank} bookHref={bookHref} locale={locale} />;
    case 'disbursed':
      return <DisbursedContext picture={picture} receipt={receipt} receiptHref={receiptHref} bookBank={bookBank} hasGlConfirmed={hasGlConfirmed} locale={locale} />;
    default:
      return null;
  }
}

function SubmissionContext({
  picture,
  receipt,
  receiptHref,
  bookBank,
  bookHref,
  locale,
}: {
  picture: ExpenseFullPicture;
  receipt: WaybillSlip | null;
  receiptHref: string | null;
  bookBank: WaybillSlip | null;
  bookHref: string | null;
  locale: SecondaryLocale;
}) {
  const { expense, items } = picture;
  const fmtDate = (d: Date | string | null) => formatDateServer(d, locale);
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, locale);
  return (
    <ContextShell
      tone="cyan"
      badge="📤 Submission package"
      hint={<Bilingual en="Everything the submitter attached" th="ข้อมูลทั้งหมดที่ผู้ส่งแนบ" locale={locale} />}
    >
      <div className="flex flex-wrap items-start gap-4">
        {receipt ? (
          <SlipRow emoji="📄" label={<Bilingual en="Receipt" th="ใบเสร็จ" locale={locale} />} slip={receipt} href={receiptHref} locale={locale} />
        ) : (
          <Field
            label={<Bilingual en="Receipt" th="ใบเสร็จ" locale={locale} />}
            value="—"
            mono
          />
        )}
        <dl className="grid flex-1 grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4">
          <Field label={<Bilingual en="Created by (Vendor)" th="ผู้ขาย" locale={locale} />} value={expense.vendor_name ?? '—'} mono />
          <Field label={<Bilingual en="Vendor Address" th="ที่อยู่ผู้ขาย" locale={locale} />} value={expense.vendor_address ?? '—'} mono />
          <Field label={<Bilingual en="Created to (Customer)" th="ลูกค้า" locale={locale} />} value={expense.created_to ?? '—'} mono />
          <Field label={<Bilingual en="Customer Address" th="ที่อยู่ลูกค้า" locale={locale} />} value={expense.created_to_address ?? '—'} mono />
          <Field label={<Bilingual en="Date" th="วันที่" locale={locale} />} value={fmtDate(expense.transaction_date)} mono />
          <Field label={<Bilingual en="Payment" th="การชำระ" locale={locale} />} value={paymentLabel(expense.payment_method)} mono />
          <Field label={<Bilingual en="Items" th="รายการ" locale={locale} />} value={`${items.length}`} mono />
          <Field label={<Bilingual en="Subtotal" th="ยอดก่อน VAT" locale={locale} />} value={fmtMoney(expense.subtotal)} mono />
          <Field label={<Bilingual en="VAT" th="VAT" locale={locale} />} value={fmtMoney(expense.vat_amount)} mono />
          <Field
            label={<Bilingual en="Total" th="รวม" locale={locale} />}
            value={fmtMoney(expense.total_amount)}
            mono
            accent="text-emerald-300 font-bold"
          />
        </dl>
      </div>
      {expense.payment_method === 'transfer' && bookBank && bookHref && (
        <div className="rounded-xl border border-indigo-500/30 bg-indigo-950/20 p-3 text-xs">
          <div className="flex items-center justify-between gap-2 text-sm font-mono uppercase tracking-widest text-indigo-200">
            <span>🏦 {<Bilingual en="Book bank (transfer)" th="สมุดบัญชี (โอน)" locale={locale} />}</span>
            <a href={bookHref} target="_blank" rel="noreferrer" className="text-cyan-300 hover:underline">
              {<Bilingual en="open slip ↗" th="เปิดสลิป ↗" locale={locale} />}
            </a>
          </div>
          <div className="mt-2 grid flex-1 grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-4">
            <Field label="Bank" value={bookBank.bank_name ?? '—'} />
            <Field label="Branch" value={bookBank.bank_branch ?? '—'} />
            <Field label={<Bilingual en="Account #" th="เลขบัญชี" locale={locale} />} value={bookBank.account_number ?? '—'} mono accent="text-cyan-300" />
            <Field label={<Bilingual en="Name" th="ชื่อ" locale={locale} />} value={bookBank.account_name ?? '—'} />
          </div>
        </div>
      )}
    </ContextShell>
  );
}

function DeptContext({
  picture,
  receipt,
  receiptHref,
  locale,
}: {
  picture: ExpenseFullPicture;
  receipt: WaybillSlip | null;
  receiptHref: string | null;
  locale: SecondaryLocale;
}) {
  const { expense } = picture;
  const fmtDate = (d: Date | string | null) => formatDateServer(d, locale);
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, locale);
  return (
    <ContextShell
      tone="amber"
      badge="👥 Dept-level check"
      hint={
        <Bilingual en="Confirm this employee in your dept actually incurred this expense — no duplicates." th="ยืนยันว่าพนักงานในแผนกของคุณใช้จ่ายจริง ไม่ซ้ำซ้อน" locale={locale} />
      }
    >
      <div className="flex flex-wrap items-center gap-4">
        {receipt && (
          <SlipRow emoji="📄" label={<Bilingual en="Receipt" th="ใบเสร็จ" locale={locale} />} slip={receipt} href={receiptHref} locale={locale} />
        )}
        <dl className="grid flex-1 grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
          <Field label={<Bilingual en="Vendor" th="ผู้ขาย" locale={locale} />} value={expense.vendor_name ?? '—'} mono />
          <Field label={<Bilingual en="Date" th="วันที่" locale={locale} />} value={fmtDate(expense.transaction_date)} mono />
          <Field label={<Bilingual en="Payment" th="การชำระ" locale={locale} />} value={paymentLabel(expense.payment_method)} mono />
          <Field
            label={<Bilingual en="Total" th="ยอดรวม" locale={locale} />}
            value={fmtMoney(expense.total_amount)}
            mono
            accent="text-amber-200 font-bold text-base"
          />
          <Field
            label={<Bilingual en="EXP" th="EXP" locale={locale} />}
            value={`EXP-${expense.id}`}
            mono
            accent="text-cyan-300"
          />
          <Field
            label={<Bilingual en="Submitter" th="ส่งโดย" locale={locale} />}
            value={expense.submitter_id ? `#${expense.submitter_id}` : '—'}
            mono
          />
        </dl>
      </div>
      <p className="text-sm font-mono text-slate-500">
        {<Bilingual en="💡 Verify the slip, amount, vendor, date — line-item breakdown is in the audit section below." th="💡 ตรวจสลิป, ยอด, ผู้ขาย, วันที่ — รายการย่อยดูได้ที่แท็บ Line items ด้านล่าง" locale={locale} />}
      </p>
    </ContextShell>
  );
}

function AccountingContext({
  picture,
  locale,
}: {
  picture: ExpenseFullPicture;
  locale: SecondaryLocale;
}) {
  const { expense, items } = picture;
  const subtotal = parseFloat(expense.subtotal ?? '0') || 0;
  const vat = parseFloat(expense.vat_amount ?? '0') || 0;
  const total = parseFloat(expense.total_amount ?? '0') || 0;
  const vatPct = subtotal > 0 ? ((vat / subtotal) * 100).toFixed(2) : '—';
  const sumItems = items.reduce((acc, it) => acc + (parseFloat(it.amount) || 0), 0);
  const diff = (total - (subtotal + vat)).toFixed(2);
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, locale);
  return (
    <ContextShell
      tone="indigo"
      badge="🧮 Accounting review"
      hint={
        <Bilingual en="Validate line items, mapped accounts, and VAT math." th="ตรวจสอบรายการย่อย, บัญชีที่ map, และภาษี" locale={locale} />
      }
    >
      <div className="grid gap-3 sm:grid-cols-4">
        <Field
          label={<Bilingual en="Subtotal" th="ยอดก่อน VAT" locale={locale} />}
          value={fmtMoney(expense.subtotal)}
          mono
        />
        <Field label={<Bilingual en="VAT" th="VAT" locale={locale} />} value={`${fmtMoney(expense.vat_amount)} (${vatPct}%)`} mono />
        <Field
          label={<Bilingual en="Total" th="ผลรวม" locale={locale} />}
          value={fmtMoney(expense.total_amount)}
          mono
          accent="text-indigo-200 font-bold"
        />
        <Field
          label={<Bilingual en="Sub+VAT vs Total" th="ส่วนต่าง" locale={locale} />}
          value={`${diff}`}
          mono
          accent={parseFloat(diff) === 0 ? 'text-emerald-300' : 'text-rose-300'}
        />
      </div>

      <div>
        <div className="flex items-center justify-between text-sm font-mono uppercase tracking-widest text-slate-500">
          <span>{<Bilingual en="Line items" th="รายการย่อย" locale={locale} />} ({items.length})</span>
          <span>
            {<Bilingual en="Σ" th="ผลรวม" locale={locale} />}{' '}
            <span className={Math.abs(sumItems - subtotal) < 0.01 ? 'text-emerald-300' : 'text-rose-300'}>
              {fmtMoney(sumItems.toString())}
            </span>
          </span>
        </div>
        {items.length === 0 ? (
          <p className="mt-1 text-xs italic text-slate-500">
            {<Bilingual en="no items — using single amount from slip" th="ไม่มีรายการ — ใช้ยอดเดียวจากใบเสร็จ" locale={locale} />}
          </p>
        ) : (
          <ul className="glass-panel mt-2 divide-y divide-slate-800/40 rounded-xl border">
            {items.map((it) => (
              <li key={it.id} className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium text-slate-100">{it.description}</div>
                  <div className="mt-0.5 flex flex-wrap gap-x-2 font-mono text-sm text-slate-500">
                    {it.qty && it.unit_price && (
                      <span>
                        {it.qty} × {fmtMoney(it.unit_price)}
                      </span>
                    )}
                    {it.mapped_account_code && (
                      <span>
                        acct: <span className="text-cyan-300">{it.mapped_account_code}</span>
                      </span>
                    )}
                  </div>
                </div>
                <div className="shrink-0 font-mono text-sm font-semibold text-cyan-200">
                  {fmtMoney(it.amount)}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </ContextShell>
  );
}

function FinalContext({
  picture,
  bookBank,
  bookHref,
  locale,
}: {
  picture: ExpenseFullPicture;
  bookBank: WaybillSlip | null;
  bookHref: string | null;
  locale: SecondaryLocale;
}) {
  const { expense } = picture;
  const fmtDate = (d: Date | string | null) => formatDateServer(d, locale);
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, locale);
  return (
    <ContextShell
      tone="emerald"
      badge="🔒 Final sign-off"
      hint={
        <Bilingual en="Final figure to post. Final approve= GL post · Final reject= close without posting." th="ตัวเลขสุดท้ายที่จะบันทึกบัญชี — กด Final approve= บันทึก, Final reject= ปิดไม่บันทึก" locale={locale} />
      }
    >
      <div className="grid gap-3 sm:grid-cols-4">
        <Field
          label={<Bilingual en="Total" th="ยอดรวม" locale={locale} />}
          value={fmtMoney(expense.total_amount)}
          mono
          accent="text-emerald-300 text-2xl font-extrabold"
        />
        <Field label={<Bilingual en="Vendor" th="ผู้ขาย" locale={locale} />} value={expense.vendor_name ?? '—'} mono />
        <Field label={<Bilingual en="Date" th="วันที่" locale={locale} />} value={fmtDate(expense.transaction_date)} mono />
        <Field
          label={<Bilingual en="Payment" th="การชำระ" locale={locale} />}
          value={paymentLabel(expense.payment_method)}
          mono
        />
      </div>
      {expense.payment_method === 'transfer' && bookBank && bookHref && (
        <div className="rounded-xl border border-emerald-500/30 bg-emerald-950/15 p-3 text-xs">
          <div className="flex items-center justify-between gap-2 text-sm font-mono uppercase tracking-widest text-emerald-200">
            <span>🏦 {<Bilingual en="Transfer to" th="โอนไปที่" locale={locale} />}</span>
            <a href={bookHref} target="_blank" rel="noreferrer" className="text-cyan-300 hover:underline">
              {<Bilingual en="open slip ↗" th="เปิดสลิป ↗" locale={locale} />}
            </a>
          </div>
          <div className="mt-2 grid flex-1 grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-4">
            <Field label="Bank" value={bookBank.bank_name ?? '—'} />
            <Field label={<Bilingual en="Branch" th="สาขา" locale={locale} />} value={bookBank.bank_branch ?? '—'} />
            <Field
              label={<Bilingual en="Account #" th="เลขบัญชี" locale={locale} />}
              value={bookBank.account_number ?? '—'}
              mono
              accent="text-emerald-300"
            />
            <Field label={<Bilingual en="Account name" th="ชื่อบัญชี" locale={locale} />} value={bookBank.account_name ?? '—'} />
          </div>
        </div>
      )}
    </ContextShell>
  );
}

function AwaitingDisbursementContext({
  picture,
  bookBank,
  bookHref,
  locale,
}: {
  picture: ExpenseFullPicture;
  bookBank: WaybillSlip | null;
  bookHref: string | null;
  locale: SecondaryLocale;
}) {
  const { expense } = picture;
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, locale);
  return (
    <ContextShell
      tone="cyan"
      badge="💸 Disbursement"
      hint={
        <Bilingual en="Attach the payment slip and pick the payout method." th="แนบสลิปการจ่ายและเลือกวิธีการจ่าย" locale={locale} />
      }
    >
      <div className="grid gap-3 sm:grid-cols-3">
        <Field
          label={<Bilingual en="Amount to pay" th="ยอดที่ต้องจ่าย" locale={locale} />}
          value={fmtMoney(expense.total_amount)}
          mono
          accent="text-cyan-200 text-xl font-extrabold"
        />
        <Field
          label={<Bilingual en="Requested method" th="วิธีจ่ายที่ขอ" locale={locale} />}
          value={paymentLabel(expense.payment_method)}
          mono
        />
        <Field
          label={<Bilingual en="EXP" th="EXP" locale={locale} />}
          value={`EXP-${expense.id}`}
          mono
          accent="text-cyan-300"
        />
      </div>
      {expense.payment_method === 'transfer' && bookBank && bookHref ? (
        <div className="rounded-xl border border-cyan-500/40 bg-cyan-950/20 p-4">
          <div className="flex items-center justify-between gap-2 text-sm font-mono uppercase tracking-widest text-cyan-200">
            <span>🏦 {<Bilingual en="Wire to" th="โอนไปที่" locale={locale} />}</span>
            <a href={bookHref} target="_blank" rel="noreferrer" className="text-cyan-300 hover:underline">
              {<Bilingual en="view original slip ↗" th="เปิดสลิปต้นฉบับ ↗" locale={locale} />}
            </a>
          </div>
          <div className="mt-3 grid flex-1 grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="sm:col-span-2">
              <div className="text-sm font-mono uppercase tracking-widest text-slate-500">
                {<Bilingual en="Account name" th="ชื่อบัญชี" locale={locale} />}
              </div>
              <div className="mt-0.5 truncate text-base font-bold text-white" title={bookBank.account_name ?? ''}>
                {bookBank.account_name ?? '—'}
              </div>
            </div>
            <div>
              <div className="text-sm font-mono uppercase tracking-widest text-slate-500">
                {<Bilingual en="Bank" th="ธนาคาร" locale={locale} />}
              </div>
              <div className="mt-0.5 truncate text-base text-slate-100">
                {bookBank.bank_name ?? '—'}
              </div>
            </div>
            <div>
              <div className="text-sm font-mono uppercase tracking-widest text-slate-500">
                {<Bilingual en="Account #" th="เลขบัญชี" locale={locale} />}
              </div>
              <div className="mt-0.5 font-mono text-base font-bold text-cyan-300">
                {bookBank.account_number ?? '—'}
              </div>
            </div>
          </div>
        </div>
      ) : (
        <p className="text-xs text-slate-400">
          {<Bilingual en="⚠ no book-bank slip — pay cash / credit and attach proof." th="⚠ ไม่มีสลิปสมุดบัญชี — โอนตรงให้ผู้ส่งหรือจ่ายเงินสด" locale={locale} />}
        </p>
      )}
    </ContextShell>
  );
}

function DisbursedContext({
  picture,
  receipt,
  receiptHref,
  bookBank,
  hasGlConfirmed,
  locale,
}: {
  picture: ExpenseFullPicture;
  receipt: WaybillSlip | null;
  receiptHref: string | null;
  bookBank: WaybillSlip | null;
  hasGlConfirmed: boolean;
  locale: SecondaryLocale;
}) {
  const { expense } = picture;
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, locale);
  return (
    <ContextShell
      tone="emerald"
      badge={hasGlConfirmed ? '✅ Disbursed · GL confirmed' : '✅ Disbursed · GL posted'}
      hint={
        hasGlConfirmed
          ? <Bilingual en="Money out + GL confirmed — closed." th="เงินออกแล้ว + ยืนยันบัญชีแล้ว — ปิดรายการ" locale={locale} />
          : <Bilingual en="Money out + GL posted — waiting on an accounting officer to confirm GL." th="เงินออกแล้ว + ลงบัญชีแล้ว — รอเจ้าหน้าที่กดยืนยัน GL" locale={locale} />
      }
    >
      <div className="flex flex-wrap items-center gap-4">
        {receipt && (
          <SlipRow emoji="📄" label={<Bilingual en="Receipt" th="ใบเสร็จ" locale={locale} />} slip={receipt} href={receiptHref} locale={locale} />
        )}
        <dl className="grid flex-1 grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
          <Field
            label={<Bilingual en="Paid" th="ยอดจ่าย" locale={locale} />}
            value={fmtMoney(expense.total_amount)}
            mono
            accent="text-emerald-300 font-bold text-base"
          />
          <Field label={<Bilingual en="Method" th="การชำระ" locale={locale} />} value={paymentLabel(expense.payment_method)} mono />
          <Field label={<Bilingual en="EXP" th="EXP" locale={locale} />} value={`EXP-${expense.id}`} mono accent="text-cyan-300" />
          <Field
            label="GL post"
            value={
              <span
                className={
                  'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-sm font-mono uppercase tracking-widest ' +
                  (hasGlConfirmed
                    ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-200'
                    : 'border-amber-500/40 bg-amber-500/15 text-amber-200')
                }
              >
                <span aria-hidden>{hasGlConfirmed ? '✓' : '◌'}</span>
                <span>{hasGlConfirmed ? 'confirmed' : 'posted, awaiting confirm'}</span>
              </span>
            }
          />
          <Field
            label={<Bilingual en="Wire to" th="โอนไป" locale={locale} />}
            value={
              bookBank?.account_number ? (
                <span className="font-mono">
                  <span className="text-cyan-300">{bookBank.account_number}</span>
                  <span className="ml-1 text-slate-400">· {bookBank.account_name ?? '—'}</span>
                </span>
              ) : (
                '—'
              )
            }
          />
          <Field
            label={<Bilingual en="Receipt" th="ใบเสร็จ" locale={locale} />}
            value={receipt ? `${receipt.mime_type}` : '—'}
            mono
          />
        </dl>
      </div>
    </ContextShell>
  );
}

export async function WaybillExpenseCollapsible({
  data,
  waybillId,
  currentStage,
  locale,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const fmtDate = (d: Date | string | null) => formatDateServer(d, localeSafe);
  const fmtMoney = (n: string | number | null) => formatMoneyServer(n, localeSafe);
  const { expense, items, slips, submitter_name } = data;
  const receiptSlip = slips.find((s) => (s as any).kind === 'receipt') ?? null;
  const bookBankSlip = slips.find((s) => (s as any).kind === 'book_bank' || (s as any).kind === 'book-bank') ?? null;
  const receiptHref = receiptSlip ? await presignedOrHash(receiptSlip) : null;
  const bookHref = bookBankSlip ? await presignedOrHash(bookBankSlip) : null;

  return (
    <details className="glass-panel group rounded-2xl border" open>
      <summary className="cursor-pointer list-none px-4 py-3 [&::-webkit-details-marker]:hidden">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <span
              aria-hidden
              className="grid h-9 w-9 place-items-center rounded-xl bg-indigo-soft text-indigo ring-1 ring-indigo/40"
            >
              📦
            </span>
            <div className="flex flex-col">
              <span className="text-base font-bold text-white">
                <Bilingual en="Expense picture" th="ภาพรวมค่าใช้จ่าย" de="Auslagenübersicht" locale={localeSafe} />
              </span>
              <span className="font-mono text-sm uppercase tracking-widest text-slate-500">
                <Bilingual en="vendor · items · slips" th="ผู้ขาย · รายการ · สลิป" de="Lieferant · Positionen · Belege" locale={localeSafe} />:{' '}
                <span className="text-cyan-300">{waybillId}</span>
              </span>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {statusPill(expense.status, localeSafe)}
            <span className="font-mono text-sm uppercase tracking-wider text-slate-500 group-open:hidden">
              ▶
            </span>
            <span className="font-mono text-sm uppercase tracking-wider text-slate-500 hidden group-open:inline">
              ▼
            </span>
          </div>
        </div>
      </summary>

      <div className="space-y-6 border-t border-slate-800/60 px-4 py-4">
        <div className="grid gap-6 lg:grid-cols-2">
          <section className="space-y-4">
            <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-slate-400">
              <span aria-hidden>📋</span>
              <span>
                <Bilingual en="Expense details" th="รายละเอียดค่าใช้จ่าย" de="Auslagendetails" locale={localeSafe} />
              </span>
            </div>
            <dl className="grid grid-cols-2 gap-x-4 divide-y divide-slate-800/40 text-sm">
              <Field label={<Bilingual en="Created by (Vendor)" th="ผู้ขาย (ผู้จัดทำ)" de="Aussteller" locale={localeSafe} />} value={expense.vendor_name ?? '—'} mono />
              <Field label={<Bilingual en="Vendor Address" th="ที่อยู่ผู้ขาย" de="Adresse des Ausstellers" locale={localeSafe} />} value={expense.vendor_address ?? '—'} mono />
              <Field label={<Bilingual en="Created to (Customer)" th="ลูกค้า (ผู้รับ)" de="Empfänger" locale={localeSafe} />} value={expense.created_to ?? '—'} mono />
              <Field label={<Bilingual en="Customer Address" th="ที่อยู่ลูกค้า" de="Adresse des Empfängers" locale={localeSafe} />} value={expense.created_to_address ?? '—'} mono />
              <Field
                label={<Bilingual en="Transaction date" th="วันที่" de="Transaktionsdatum" locale={localeSafe} />}
                value={fmtDate(expense.transaction_date)}
                mono
              />
              <Field
                label={<Bilingual en="Payment method" th="การชำระ" de="Zahlungsart" locale={localeSafe} />}
                value={expense.payment_method ?? '—'}
                mono
              />
              <Field
                label={<Bilingual en="Submitter" th="ผู้ส่ง" de="Einreicher" locale={localeSafe} />}
                value={
                  submitter_name
                    ? `${submitter_name} #${expense.submitter_id ?? '—'}`
                    : expense.submitter_id
                    ? `#${expense.submitter_id}`
                    : '—'
                }
              />
              <Field
                label={<Bilingual en="Subtotal" th="ยอดก่อน VAT" de="Zwischensumme" locale={localeSafe} />}
                value={fmtMoney(expense.subtotal)}
                mono
                accent="text-slate-200 text-base"
              />
              <Field
                label={<Bilingual en="VAT" th="VAT" de="USt." locale={localeSafe} />}
                value={fmtMoney(expense.vat_amount)}
                mono
                accent="text-slate-200 text-base"
              />
              <Field
                label={<Bilingual en="Total" th="รวม" de="Gesamt" locale={localeSafe} />}
                value={fmtMoney(expense.total_amount)}
                mono
                accent="text-2xl sm:text-3xl text-emerald-300 font-extrabold"
                full
              />
            </dl>
            {expense.rejection_reason && (
              <p className="rounded-lg border border-rose-500/40 bg-rose-950/40 px-3 py-2 text-sm italic text-rose-200">
                <span aria-hidden>✗</span>{' '}
                <Bilingual en="Rejection reason" th="เหตุผลที่ปฏิเสธ" de="Ablehnungsgrund" locale={localeSafe} />: &ldquo;{expense.rejection_reason}&rdquo;
              </p>
            )}
          </section>

          <section className="space-y-4">
            <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-slate-400">
              <span aria-hidden>🧾</span>
              <span>
                <Bilingual en="Line items" th="รายการ" de="Positionen" locale={localeSafe} /> ({items.length})
              </span>
            </div>
            {items.length === 0 ? (
              <p className="text-sm italic text-slate-500">
                <Bilingual en="no line items" th="ไม่มีรายการ" de="keine Positionen" locale={localeSafe} />
              </p>
            ) : (
              <ul className="divide-y divide-slate-800/40">
                {items.map((it) => (
                  <li
                    key={it.id}
                    className="flex items-center justify-between gap-3 py-2 text-sm"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-slate-100 font-medium">{it.description}</div>
                      <div className="mt-0.5 flex flex-wrap gap-x-3 text-xs font-mono text-slate-500">
                        {it.qty && it.unit_price && (
                          <span>
                            {it.qty} × {fmtMoney(it.unit_price)}
                          </span>
                        )}
                        {it.mapped_account_code && (
                          <span>
                            acct: <span className="text-cyan-300">{it.mapped_account_code}</span>
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="shrink-0 text-right font-mono text-lg font-semibold text-cyan-200">
                      {fmtMoney(it.amount)}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        <section className="space-y-4">
          <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-slate-400">
            <span aria-hidden>📎</span>
            <span>
              <Bilingual en="Slip attachments" th="สลิปที่แนบ" de="Beleganlagen" locale={localeSafe} />
            </span>
          </div>
          <div className="grid gap-x-6 gap-y-2 sm:grid-cols-2 divide-y divide-slate-800/40 sm:divide-y-0">
            <SlipRow
              emoji="🧾"
              label={
                <Bilingual en="Receipt" th="ใบเสร็จ" de="Beleg" locale={localeSafe} />
              }
              slip={receiptSlip}
              href={receiptHref}
              locale={localeSafe}
            />
            <SlipRow
              emoji="📖"
              label={
                <Bilingual en="Book bank" th="สมุดบัญชี" de="Kontoauszug" locale={localeSafe} />
              }
              slip={bookBankSlip}
              href={bookHref}
              locale={localeSafe}
              bankFields={
                bookBankSlip
                  ? {
                      bankName: bookBankSlip.bank_name,
                      accountNumber: bookBankSlip.account_number,
                      accountName: bookBankSlip.account_name,
                      bankBranch: bookBankSlip.bank_branch,
                    }
                  : null
              }
            />
          </div>
        </section>

        <section className="space-y-4 border-t border-slate-800/60 pt-4">
          <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-slate-400">
            <span aria-hidden>🎯</span>
            <span>
              <Bilingual en="Step context" th="บริบทขั้นตอน" de="Schritt-Kontext" locale={localeSafe} /> ·{' '}
              <span className="text-cyan-300">{currentStage}</span>
            </span>
          </div>
          <StepContextBlock
            pipKey={currentStage}
            picture={data}
            hasGlConfirmed={false}
            locale={localeSafe}
          />
        </section>
      </div>
    </details>
  );
}