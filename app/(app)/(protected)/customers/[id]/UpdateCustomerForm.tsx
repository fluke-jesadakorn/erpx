'use client';
import { useState } from 'react';
import { updateCustomerAction, blacklistCustomerAction } from './_actions';
import type { CustomerRow } from '@/customer/queries';
import { T } from '@/components/i18n/T';

export function UpdateCustomerForm({ customer }: { customer: CustomerRow }) {
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
    setMsg(r.ok ? '✓ Saved' : r.error ?? 'failed');
  }

  async function toggleBlacklist() {
    setBusy(true);
    const fd = new FormData();
    fd.set('id', String(customer.id));
    fd.set('blacklist', customer.blacklist ? 'false' : 'true');
    const r = await blacklistCustomerAction(fd);
    setBusy(false);
    setMsg(r.ok ? '✓ Updated' : r.error ?? 'failed');
    if (r.ok) setTimeout(() => location.reload(), 200);
  }

  return (
    <form onSubmit={submit} className="space-y-3">
      <input type="hidden" name="id" value={customer.id} />
      <Field label={<T id="customers.formName" />} name="name" defaultValue={customer.name} required />
      <Field label={<T id="customers.formNameTh" />} name="name_th" defaultValue={customer.name_th ?? ''} />
      <Field label={<T id="customers.formTaxId" />} name="tax_id" defaultValue={customer.tax_id ?? ''} />
      <Field label={<T id="customers.formBillingAddress" />} name="billing_address" defaultValue={customer.billing_address ?? ''} multiline />
      <Field label={<T id="customers.formShippingAddress" />} name="shipping_address" defaultValue={customer.shipping_address ?? ''} multiline />
      <Field label={<T id="customers.formContactName" />} name="contact_name" defaultValue={customer.contact_name ?? ''} />
      <Field label={<T id="customers.formContactEmail" />} name="contact_email" type="email" defaultValue={customer.contact_email ?? ''} />
      <Field label={<T id="customers.formContactPhone" />} name="contact_phone" defaultValue={customer.contact_phone ?? ''} />
      <Field label={<T id="customers.formCreditLimit" />} name="credit_limit_thb" type="number" defaultValue={String(customer.credit_limit_thb)} />
      <Field label={<T id="customers.formPaymentTerms" />} name="payment_terms" defaultValue={customer.payment_terms} />

      <div className="flex items-center gap-3 pt-2">
        <button
          type="submit"
          disabled={busy}
          className="rounded-lg border border-info/40 bg-info px-3 py-1.5 text-xs font-mono text-info hover:bg-info disabled:opacity-50"
        >
          <T id="customers.formSave" />
        </button>
        <button
          type="button"
          onClick={toggleBlacklist}
          disabled={busy}
          className="rounded-lg border border-critical/40 bg-critical px-3 py-1.5 text-xs font-mono text-critical hover:bg-critical disabled:opacity-50"
        >
          <T id={customer.blacklist ? 'customers.formUnblock' : 'customers.formBlacklist'} />
        </button>
        {msg ? <span className="text-sm font-mono text-ink-2">{msg}</span> : null}
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
      <span className="text-xs font-mono uppercase tracking-wider text-mute">{label}</span>
      {multiline ? (
        <textarea
          name={name}
          defaultValue={defaultValue}
          rows={2}
          required={required}
          className="mt-1 block w-full rounded-lg bg-paper border border-rule px-3 py-2 text-sm text-ink focus:outline-none focus:border-accent/40"
        />
      ) : (
        <input
          name={name}
          type={type}
          defaultValue={defaultValue}
          required={required}
          className="mt-1 block w-full rounded-lg bg-paper border border-rule px-3 py-2 text-sm text-ink focus:outline-none focus:border-accent/40"
        />
      )}
    </label>
  );
}