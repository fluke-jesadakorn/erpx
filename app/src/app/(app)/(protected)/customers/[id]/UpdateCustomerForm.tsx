'use client';
import { useState } from 'react';
import { updateCustomerAction, blacklistCustomerAction } from './_actions';
import type { CustomerRow } from '@folio-lib/customer/queries';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { Bilingual } from '@/components/i18n/Bilingual';
import custDict from '@folio-lib/i18n/customers';

function t(key: string, locale: 'th' | 'de'): string {
  const en = custDict.en[key] ?? key;
  if (locale === 'de') return custDict.de?.[key] ?? en;
  return custDict.th?.[key] ?? en;
}

export function UpdateCustomerForm({ customer }: { customer: CustomerRow }) {
  const locale = useSecondaryLocale();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (busy) return;
    const fd = new FormData(e.currentTarget);
    setBusy(true);
    setMsg(null);
    const r = await updateCustomerAction(fd);
    setBusy(false);
    setMsg(r.ok ? t('customers.form.saved', locale) : r.error ?? t('customers.form.failed', locale));
  }

  async function toggleBlacklist() {
    setBusy(true);
    const fd = new FormData();
    fd.set('id', String(customer.id));
    fd.set('blacklist', customer.blacklist ? 'false' : 'true');
    const r = await blacklistCustomerAction(fd);
    setBusy(false);
    setMsg(r.ok ? t('customers.form.updated', locale) : r.error ?? t('customers.form.failed', locale));
    if (r.ok) setTimeout(() => location.reload(), 200);
  }

  return (
    <form onSubmit={submit} className="space-y-3">
      <input type="hidden" name="id" value={customer.id} />
      <Field
        label={<Bilingual en="Name" th="ชื่อ" de="Name" locale={locale} />}
        name="name"
        defaultValue={customer.name}
        required
      />
      <Field
        label={<Bilingual en="Name (TH)" th="ชื่อ (TH)" de="Name (TH)" locale={locale} />}
        name="name_th"
        defaultValue={customer.name_th ?? ''}
      />
      <Field
        label={<Bilingual en="Tax ID" th="เลขประจำตัวผู้เสียภาษี" de="Steuernummer" locale={locale} />}
        name="tax_id"
        defaultValue={customer.tax_id ?? ''}
      />
      <Field
        label={<Bilingual en="Billing address" th="ที่อยู่เรียกเก็บเงิน" de="Rechnungsadresse" locale={locale} />}
        name="billing_address"
        defaultValue={customer.billing_address ?? ''}
        multiline
      />
      <Field
        label={<Bilingual en="Shipping address" th="ที่อยู่จัดส่ง" de="Lieferadresse" locale={locale} />}
        name="shipping_address"
        defaultValue={customer.shipping_address ?? ''}
        multiline
      />
      <Field
        label={<Bilingual en="Contact name" th="ชื่อผู้ติดต่อ" de="Kontaktperson" locale={locale} />}
        name="contact_name"
        defaultValue={customer.contact_name ?? ''}
      />
      <Field
        label={<Bilingual en="Contact email" th="อีเมลผู้ติดต่อ" de="Kontakt-E-Mail" locale={locale} />}
        name="contact_email"
        type="email"
        defaultValue={customer.contact_email ?? ''}
      />
      <Field
        label={<Bilingual en="Contact phone" th="โทรศัพท์ผู้ติดต่อ" de="Kontakt-Telefon" locale={locale} />}
        name="contact_phone"
        defaultValue={customer.contact_phone ?? ''}
      />
      <Field
        label={<Bilingual en="Credit limit (THB)" th="วงเงินเครดิต (THB)" de="Kreditlimit (THB)" locale={locale} />}
        name="credit_limit_thb"
        type="number"
        defaultValue={String(customer.credit_limit_thb)}
      />
      <Field
        label={<Bilingual en="Payment terms" th="เงื่อนไขการชำระ" de="Zahlungsbedingungen" locale={locale} />}
        name="payment_terms"
        defaultValue={customer.payment_terms}
      />

      <div className="flex items-center gap-3 pt-2">
        <button
          type="submit"
          disabled={busy}
          className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-mono text-cyan-200 hover:bg-cyan-500/20 disabled:opacity-50"
        >
          {t('customers.form.save', locale)}
        </button>
        <button
          type="button"
          onClick={toggleBlacklist}
          disabled={busy}
          className="rounded-lg border border-rose-500/40 bg-rose-500/10 px-3 py-1.5 text-xs font-mono text-rose-200 hover:bg-rose-500/20 disabled:opacity-50"
        >
          {customer.blacklist
            ? <Bilingual en="✓ Unblock" th="✓ ปลดบล็อค" de="✓ Entsperren" locale={locale} />
            : <Bilingual en="⛔ Blacklist" th="⛔ บล็อคลิสต์" de="⛔ Sperren" locale={locale} />}
        </button>
        {msg ? <span className="text-sm font-mono text-slate-400">{msg}</span> : null}
      </div>
    </form>
  );
}

function Field({
  label, name, defaultValue, type = 'text', required = false, multiline = false,
}: {
  label: React.ReactNode;
  name: string;
  defaultValue?: string;
  type?: string;
  required?: boolean;
  multiline?: boolean;
}) {
  return (
    <label className="block">
      <span className="text-xs font-mono uppercase tracking-wider text-slate-500">{label}</span>
      {multiline ? (
        <textarea
          name={name}
          defaultValue={defaultValue}
          rows={2}
          required={required}
          className="mt-1 block w-full rounded-lg bg-slate-900 border border-slate-700 px-3 py-2 text-sm text-white focus:outline-none focus:border-indigo-500"
        />
      ) : (
        <input
          name={name}
          type={type}
          defaultValue={defaultValue}
          required={required}
          className="mt-1 block w-full rounded-lg bg-slate-900 border border-slate-700 px-3 py-2 text-sm text-white focus:outline-none focus:border-indigo-500"
        />
      )}
    </label>
  );
}
