import React from 'react';
import type { SecondaryLocale } from '@/server/locale';
import type {
  ExpenseFullPicture,
  WaybillSlip,
} from '@/waybill/queries';
import { fmtSize, statusPill } from './ui';
import { SlipThumbZoom } from './SlipThumbZoom';
import { formatDateServer, formatMoneyServer } from '@/components/i18n/formattersServer';
import { T } from '@/components/i18n/T';

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
  locale: _locale,
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
        <span className="text-sm font-bold text-ink">{label}</span>
        {slip ? (
          <span className="ml-auto rounded-full border border-info bg-info px-2 py-0.5 text-sm font-mono uppercase tracking-widest text-info-soft">
            ✓ {slip.status}
          </span>
        ) : (
          <span className="bg-paper-2 border border-rule ml-auto rounded-full border px-2 py-0.5 text-sm font-mono uppercase tracking-widest text-mute">
            <T id="waybill.expense.none" />
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
              className="block break-all font-mono text-sm font-semibold text-info underline-offset-2 hover:underline"
            >
              {slip.file_path.split('/').slice(-1)[0]}
            </a>
            <div className="text-xs font-mono text-mute">
              {fmtSize(slip.file_size)} · {slip.mime_type}
            </div>
            {bankFields && (bankFields.bankName || bankFields.accountNumber) && (
              <div className="text-xs font-mono text-ink-2">
                {bankFields.bankName && (
                  <span className="mr-3">
                    🏦 <span className="text-ink">{bankFields.bankName}</span>
                  </span>
                )}
                {bankFields.accountNumber && (
                  <span className="mr-3">
                    #<span className="text-info font-bold">{bankFields.accountNumber}</span>
                  </span>
                )}
                {bankFields.accountName && (
                  <span>
                    👤 <span className="text-ink">{bankFields.accountName}</span>
                  </span>
                )}
              </div>
            )}
          </div>
        </div>
      ) : (
        <p className="mt-1.5 text-xs italic text-mute">
          <T id="waybill.expense.not_attached" />
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
      <dt className="text-sm font-mono uppercase tracking-widest text-mute">{label}</dt>
      <dd
        className={
          'mt-0.5 text-sm text-ink' +
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
    cyan: 'border-info bg-info-strong',
    amber: 'border-caution bg-caution-strong',
    indigo: 'border-accent bg-accent-strong',
    emerald: 'border-positive bg-positive-strong',
    rose: 'border-critical bg-critical-strong',
    slate: 'bg-paper-2 border border-rule',
  };
  const headerCls: Record<typeof tone, string> = {
    cyan: 'text-info-soft',
    amber: 'text-caution-soft',
    indigo: 'text-accent-soft',
    emerald: 'text-positive-soft',
    rose: 'text-critical-soft',
    slate: 'text-ink',
  };
  return (
    <section className={'space-y-3 rounded-md border p-4 ' + toneCls[tone]}>
      <header className="flex flex-wrap items-center gap-2 text-sm font-mono uppercase tracking-widest">
        <span className={headerCls[tone]}>{badge}</span>
      </header>
      {hint && <p className="text-sm italic text-ink-2">{hint}</p>}
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
      hint={<T id="waybill.expense.everything_the_submitter_attached" />}
    >
      <div className="flex flex-wrap items-start gap-4">
        {receipt ? (
          <SlipRow emoji="📄" label={<T id="waybill.expense.receipt" />} slip={receipt} href={receiptHref} locale={locale} />
        ) : (
          <Field
            label={<T id="waybill.expense.receipt" />}
            value="—"
            mono
          />
        )}
        <dl className="grid flex-1 grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4">
          <Field label={<T id="waybill.expense.created_by_vendor" />} value={expense.vendor_name ?? '—'} mono />
          <Field label={<T id="waybill.expense.vendor_address" />} value={expense.vendor_address ?? '—'} mono />
          <Field label={<T id="waybill.expense.created_to_customer" />} value={expense.created_to ?? '—'} mono />
          <Field label={<T id="waybill.expense.customer_address" />} value={expense.created_to_address ?? '—'} mono />
          <Field label={<T id="waybill.expense.date" />} value={fmtDate(expense.transaction_date)} mono />
          <Field label={<T id="waybill.expense.payment" />} value={paymentLabel(expense.payment_method)} mono />
          <Field label={<T id="waybill.expense.items" />} value={`${items.length}`} mono />
          <Field label={<T id="waybill.expense.subtotal" />} value={fmtMoney(expense.subtotal)} mono />
          <Field label={<T id="waybill.expense.vat" />} value={fmtMoney(expense.vat_amount)} mono />
          <Field
            label={<T id="waybill.expense.total" />}
            value={fmtMoney(expense.total_amount)}
            mono
            accent="text-positive font-bold"
          />
        </dl>
      </div>
      {expense.payment_method === 'transfer' && bookBank && bookHref && (
        <div className="rounded-md border border-accent bg-accent-soft p-3 text-xs">
          <div className="flex items-center justify-between gap-2 text-sm font-mono uppercase tracking-widest text-accent-soft">
            <span>🏦 {<T id="waybill.expense.book_bank_transfer" />}</span>
            <a href={bookHref} target="_blank" rel="noreferrer" className="text-info hover:underline">
              {<T id="waybill.expense.open_slip" />}
            </a>
          </div>
          <div className="mt-2 grid flex-1 grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-4">
            <Field label="Bank" value={bookBank.bank_name ?? '—'} />
            <Field label="Branch" value={bookBank.bank_branch ?? '—'} />
            <Field label={<T id="waybill.expense.account" />} value={bookBank.account_number ?? '—'} mono accent="text-info" />
            <Field label={<T id="waybill.expense.name" />} value={bookBank.account_name ?? '—'} />
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
        <T id="waybill.expense.confirm_this_employee_in_your_dep_2c6e32" />
      }
    >
      <div className="flex flex-wrap items-center gap-4">
        {receipt && (
          <SlipRow emoji="📄" label={<T id="waybill.expense.receipt" />} slip={receipt} href={receiptHref} locale={locale} />
        )}
        <dl className="grid flex-1 grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
          <Field label={<T id="waybill.expense.vendor" />} value={expense.vendor_name ?? '—'} mono />
          <Field label={<T id="waybill.expense.date" />} value={fmtDate(expense.transaction_date)} mono />
          <Field label={<T id="waybill.expense.payment" />} value={paymentLabel(expense.payment_method)} mono />
          <Field
            label={<T id="waybill.expense.total" />}
            value={fmtMoney(expense.total_amount)}
            mono
            accent="text-caution-soft font-bold text-base"
          />
          <Field
            label={<T id="waybill.expense.exp" />}
            value={`EXP-${expense.id}`}
            mono
            accent="text-info"
          />
          <Field
            label={<T id="waybill.expense.submitter" />}
            value={expense.submitter_id ? `#${expense.submitter_id}` : '—'}
            mono
          />
        </dl>
      </div>
      <p className="text-sm font-mono text-mute">
        {<T id="waybill.expense.verify_the_slip_amount_vendor_dat_fdfd09" />}
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
        <T id="waybill.expense.validate_line_items_mapped_accoun_d38301" />
      }
    >
      <div className="grid gap-3 sm:grid-cols-4">
        <Field
          label={<T id="waybill.expense.subtotal" />}
          value={fmtMoney(expense.subtotal)}
          mono
        />
        <Field label={<T id="waybill.expense.vat" />} value={`${fmtMoney(expense.vat_amount)} (${vatPct}%)`} mono />
        <Field
          label={<T id="waybill.expense.total" />}
          value={fmtMoney(expense.total_amount)}
          mono
          accent="text-accent-soft font-bold"
        />
        <Field
          label={<T id="waybill.expense.sub_vat_vs_total" />}
          value={`${diff}`}
          mono
          accent={parseFloat(diff) === 0 ? 'text-positive' : 'text-critical'}
        />
      </div>

      <div>
        <div className="flex items-center justify-between text-sm font-mono uppercase tracking-widest text-mute">
          <span>{<T id="waybill.expense.line_items" />} ({items.length})</span>
          <span>
            {<T id="waybill.expense.x" />}{' '}
            <span className={Math.abs(sumItems - subtotal) < 0.01 ? 'text-positive' : 'text-critical'}>
              {fmtMoney(sumItems.toString())}
            </span>
          </span>
        </div>
        {items.length === 0 ? (
          <p className="mt-1 text-xs italic text-mute">
            {<T id="waybill.expense.no_items_using_single_amount_from_slip" />}
          </p>
        ) : (
          <ul className="bg-paper-2 border border-rule mt-2 divide-y divide-rule rounded-md border">
            {items.map((it) => (
              <li key={it.id} className="flex items-center justify-between gap-3 px-3 py-2 text-xs">
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium text-ink">{it.description}</div>
                  <div className="mt-0.5 flex flex-wrap gap-x-2 font-mono text-sm text-mute">
                    {it.qty && it.unit_price && (
                      <span>
                        {it.qty} × {fmtMoney(it.unit_price)}
                      </span>
                    )}
                    {it.mapped_account_code && (
                      <span>
                        acct: <span className="text-info">{it.mapped_account_code}</span>
                      </span>
                    )}
                  </div>
                </div>
                <div className="shrink-0 font-mono text-sm font-semibold text-info-soft">
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
        <T id="waybill.expense.final_figure_to_post_final_approv_1b22b3" />
      }
    >
      <div className="grid gap-3 sm:grid-cols-4">
        <Field
          label={<T id="waybill.expense.total" />}
          value={fmtMoney(expense.total_amount)}
          mono
          accent="text-positive text-2xl font-extrabold"
        />
        <Field label={<T id="waybill.expense.vendor" />} value={expense.vendor_name ?? '—'} mono />
        <Field label={<T id="waybill.expense.date" />} value={fmtDate(expense.transaction_date)} mono />
        <Field
          label={<T id="waybill.expense.payment" />}
          value={paymentLabel(expense.payment_method)}
          mono
        />
      </div>
      {expense.payment_method === 'transfer' && bookBank && bookHref && (
        <div className="rounded-md border border-positive bg-positive-soft p-3 text-xs">
          <div className="flex items-center justify-between gap-2 text-sm font-mono uppercase tracking-widest text-positive-soft">
            <span>🏦 {<T id="waybill.expense.transfer_to" />}</span>
            <a href={bookHref} target="_blank" rel="noreferrer" className="text-info hover:underline">
              {<T id="waybill.expense.open_slip" />}
            </a>
          </div>
          <div className="mt-2 grid flex-1 grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-4">
            <Field label="Bank" value={bookBank.bank_name ?? '—'} />
            <Field label={<T id="waybill.expense.branch" />} value={bookBank.bank_branch ?? '—'} />
            <Field
              label={<T id="waybill.expense.account" />}
              value={bookBank.account_number ?? '—'}
              mono
              accent="text-positive"
            />
            <Field label={<T id="waybill.expense.account_name" />} value={bookBank.account_name ?? '—'} />
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
        <T id="waybill.expense.attach_the_payment_slip_and_pick__e31664" />
      }
    >
      <div className="grid gap-3 sm:grid-cols-3">
        <Field
          label={<T id="waybill.expense.amount_to_pay" />}
          value={fmtMoney(expense.total_amount)}
          mono
          accent="text-info-soft text-xl font-extrabold"
        />
        <Field
          label={<T id="waybill.expense.requested_method" />}
          value={paymentLabel(expense.payment_method)}
          mono
        />
        <Field
          label={<T id="waybill.expense.exp" />}
          value={`EXP-${expense.id}`}
          mono
          accent="text-info"
        />
      </div>
      {expense.payment_method === 'transfer' && bookBank && bookHref ? (
        <div className="rounded-md border border-info bg-info-soft p-4">
          <div className="flex items-center justify-between gap-2 text-sm font-mono uppercase tracking-widest text-info-soft">
            <span>🏦 {<T id="waybill.expense.wire_to" />}</span>
            <a href={bookHref} target="_blank" rel="noreferrer" className="text-info hover:underline">
              {<T id="waybill.expense.view_original_slip" />}
            </a>
          </div>
          <div className="mt-3 grid flex-1 grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="sm:col-span-2">
              <div className="text-sm font-mono uppercase tracking-widest text-mute">
                {<T id="waybill.expense.account_name" />}
              </div>
              <div className="mt-0.5 truncate text-base font-bold text-ink" title={bookBank.account_name ?? ''}>
                {bookBank.account_name ?? '—'}
              </div>
            </div>
            <div>
              <div className="text-sm font-mono uppercase tracking-widest text-mute">
                {<T id="waybill.expense.bank" />}
              </div>
              <div className="mt-0.5 truncate text-base text-ink">
                {bookBank.bank_name ?? '—'}
              </div>
            </div>
            <div>
              <div className="text-sm font-mono uppercase tracking-widest text-mute">
                {<T id="waybill.expense.account" />}
              </div>
              <div className="mt-0.5 font-mono text-base font-bold text-info">
                {bookBank.account_number ?? '—'}
              </div>
            </div>
          </div>
        </div>
      ) : (
        <p className="text-xs text-ink-2">
          {<T id="waybill.expense.no_book_bank_slip_pay_cash_credit_7adb70" />}
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
          ? <T id="waybill.expense.money_out_gl_confirmed_closed" />
          : <T id="waybill.expense.money_out_gl_posted_waiting_on_an_0272fd" />
      }
    >
      <div className="flex flex-wrap items-center gap-4">
        {receipt && (
          <SlipRow emoji="📄" label={<T id="waybill.expense.receipt" />} slip={receipt} href={receiptHref} locale={locale} />
        )}
        <dl className="grid flex-1 grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
          <Field
            label={<T id="waybill.expense.paid" />}
            value={fmtMoney(expense.total_amount)}
            mono
            accent="text-positive font-bold text-base"
          />
          <Field label={<T id="waybill.expense.method" />} value={paymentLabel(expense.payment_method)} mono />
          <Field label={<T id="waybill.expense.exp" />} value={`EXP-${expense.id}`} mono accent="text-info" />
          <Field
            label="GL post"
            value={
              <span
                className={
                  'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-sm font-mono uppercase tracking-widest ' +
                  (hasGlConfirmed
                    ? 'border-positive bg-positive text-paper'
                    : 'border-caution bg-caution-soft text-caution-strong border border-caution')
                }
              >
                <span aria-hidden>{hasGlConfirmed ? '✓' : '◌'}</span>
                <span>{hasGlConfirmed ? 'confirmed' : 'posted, awaiting confirm'}</span>
              </span>
            }
          />
          <Field
            label={<T id="waybill.expense.wire_to" />}
            value={
              bookBank?.account_number ? (
                <span className="font-mono">
                  <span className="text-info">{bookBank.account_number}</span>
                  <span className="ml-1 text-ink-2">· {bookBank.account_name ?? '—'}</span>
                </span>
              ) : (
                '—'
              )
            }
          />
          <Field
            label={<T id="waybill.expense.receipt" />}
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
    <details className="bg-paper-2 border border-rule group rounded-md border" open>
      <summary className="cursor-pointer list-none px-4 py-3 [&::-webkit-details-marker]:hidden">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <span
              aria-hidden
              className="grid h-9 w-9 place-items-center rounded-md bg-indigo-soft text-indigo ring-1 ring-indigo/40"
            >
              📦
            </span>
            <div className="flex flex-col">
              <span className="text-base font-bold text-ink">
                <T id="waybill.expense.expense_picture" />
              </span>
              <span className="font-mono text-sm uppercase tracking-widest text-mute">
                <T id="waybill.expense.vendor_items_slips" />:{' '}
                <span className="text-info">{waybillId}</span>
              </span>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {statusPill(expense.status, localeSafe)}
            <span className="font-mono text-sm uppercase tracking-wider text-mute group-open:hidden">
              ▶
            </span>
            <span className="font-mono text-sm uppercase tracking-wider text-mute hidden group-open:inline">
              ▼
            </span>
          </div>
        </div>
      </summary>

      <div className="space-y-6 border-t border-rule/60 px-4 py-4">
        <div className="grid gap-6 lg:grid-cols-2">
          <section className="space-y-4">
            <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-ink-2">
              <span aria-hidden>📋</span>
              <span>
                <T id="waybill.expense.expense_details" />
              </span>
            </div>
            <dl className="grid grid-cols-2 gap-x-4 divide-y divide-rule text-sm">
              <Field label={<T id="waybill.expense.created_by_vendor" />} value={expense.vendor_name ?? '—'} mono />
              <Field label={<T id="waybill.expense.vendor_address" />} value={expense.vendor_address ?? '—'} mono />
              <Field label={<T id="waybill.expense.created_to_customer" />} value={expense.created_to ?? '—'} mono />
              <Field label={<T id="waybill.expense.customer_address" />} value={expense.created_to_address ?? '—'} mono />
              <Field
                label={<T id="waybill.expense.transaction_date" />}
                value={fmtDate(expense.transaction_date)}
                mono
              />
              <Field
                label={<T id="waybill.expense.payment_method" />}
                value={expense.payment_method ?? '—'}
                mono
              />
              <Field
                label={<T id="waybill.expense.submitter" />}
                value={
                  submitter_name
                    ? `${submitter_name} #${expense.submitter_id ?? '—'}`
                    : expense.submitter_id
                    ? `#${expense.submitter_id}`
                    : '—'
                }
              />
              <Field
                label={<T id="waybill.expense.subtotal" />}
                value={fmtMoney(expense.subtotal)}
                mono
                accent="text-ink text-base"
              />
              <Field
                label={<T id="waybill.expense.vat" />}
                value={fmtMoney(expense.vat_amount)}
                mono
                accent="text-ink text-base"
              />
              <Field
                label={<T id="waybill.expense.total" />}
                value={fmtMoney(expense.total_amount)}
                mono
                accent="text-2xl sm:text-3xl text-positive font-extrabold"
                full
              />
            </dl>
            {expense.rejection_reason && (
              <p className="rounded-lg border border-critical bg-critical-strong px-3 py-2 text-sm italic text-critical-soft">
                <span aria-hidden>✗</span>{' '}
                <T id="waybill.expense.rejection_reason" />: &ldquo;{expense.rejection_reason}&rdquo;
              </p>
            )}
          </section>

          <section className="space-y-4">
            <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-ink-2">
              <span aria-hidden>🧾</span>
              <span>
                <T id="waybill.expense.line_items" /> ({items.length})
              </span>
            </div>
            {items.length === 0 ? (
              <p className="text-sm italic text-mute">
                <T id="waybill.expense.no_line_items" />
              </p>
            ) : (
              <ul className="divide-y divide-rule">
                {items.map((it) => (
                  <li
                    key={it.id}
                    className="flex items-center justify-between gap-3 py-2 text-sm"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-ink font-medium">{it.description}</div>
                      <div className="mt-0.5 flex flex-wrap gap-x-3 text-xs font-mono text-mute">
                        {it.qty && it.unit_price && (
                          <span>
                            {it.qty} × {fmtMoney(it.unit_price)}
                          </span>
                        )}
                        {it.mapped_account_code && (
                          <span>
                            acct: <span className="text-info">{it.mapped_account_code}</span>
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="shrink-0 text-right font-mono text-lg font-semibold text-info-soft">
                      {fmtMoney(it.amount)}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        <section className="space-y-4">
          <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-ink-2">
            <span aria-hidden>📎</span>
            <span>
              <T id="waybill.expense.slip_attachments" />
            </span>
          </div>
          <div className="grid gap-x-6 gap-y-2 sm:grid-cols-2 divide-y divide-rule sm:divide-y-0">
            <SlipRow
              emoji="🧾"
              label={
                <T id="waybill.expense.receipt" />
              }
              slip={receiptSlip}
              href={receiptHref}
              locale={localeSafe}
            />
            <SlipRow
              emoji="📖"
              label={
                <T id="waybill.expense.book_bank" />
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

        <section className="space-y-4 border-t border-rule/60 pt-4">
          <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-ink-2">
            <span aria-hidden>🎯</span>
            <span>
              <T id="waybill.expense.step_context" /> ·{' '}
              <span className="text-info">{currentStage}</span>
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