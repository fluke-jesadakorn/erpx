'use client';

import { useRef, useState } from 'react';

interface Account {
  id: number;
  code: string;
  bankName: string;
  currency: string;
}

type Mapping = Partial<Record<'transactionDate' | 'valueDate' | 'description' | 'reference' | 'currency' | 'amount' | 'balance', string>>;

interface Preview {
  headers: string[];
  mapping: Mapping;
  rows: Array<Record<string, unknown>>;
  duplicateFile: boolean;
  mappingComplete: boolean;
  error?: string;
}

const fields: Array<{ key: keyof Mapping; label: string; required?: boolean }> = [
  { key: 'transactionDate', label: 'Transaction date', required: true },
  { key: 'description', label: 'Description', required: true },
  { key: 'amount', label: 'Amount', required: true },
  { key: 'valueDate', label: 'Value date' },
  { key: 'reference', label: 'Reference' },
  { key: 'currency', label: 'Currency' },
  { key: 'balance', label: 'Balance' },
];

export function BankImportPanel({ accounts }: { accounts: Account[] }) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [accountId, setAccountId] = useState(accounts[0]?.id ?? 0);
  const [mapping, setMapping] = useState<Mapping>({});
  const [preview, setPreview] = useState<Preview | null>(null);
  const [templateName, setTemplateName] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');

  async function send(path: 'preview' | 'commit') {
    const file = fileRef.current?.files?.[0];
    if (!file || !accountId) return setMessage('Choose a bank account and CSV/XLSX file.');
    setBusy(true);
    setMessage('');
    const form = new FormData();
    form.set('file', file);
    form.set('bank_account_id', String(accountId));
    form.set('mapping', JSON.stringify(mapping));
    form.set('template_name', templateName);
    const response = await fetch(`/api/finance/bank-import/${path}`, { method: 'POST', body: form });
    const body = await response.json();
    setBusy(false);
    if (!response.ok) return setMessage(body.error ?? 'Import failed');
    if (path === 'preview') {
      setPreview(body);
      setMapping(body.mapping);
      setMessage(body.mappingComplete ? `Validated ${body.rows.length} row(s).` : 'Map the required columns, then preview again.');
    } else {
      setMessage(`Committed ${body.rowCount} row(s). Refreshing…`);
      window.location.reload();
    }
  }

  return <section className="panel-elevated p-5"><h2 className="text-lg font-bold">Import bank statement</h2><p className="mt-1 text-sm text-ink-2">CSV/XLSX is validated before an atomic commit. Required XLSX cells cannot contain formulas.</p>{accounts.length ? <div className="mt-4 space-y-4"><div className="grid gap-3 sm:grid-cols-2"><label className="block text-sm">Bank account<select className="field" value={accountId} onChange={(event) => setAccountId(Number(event.target.value))}>{accounts.map((account) => <option key={account.id} value={account.id}>{account.code} · {account.bankName} · {account.currency}</option>)}</select></label><label className="block text-sm">Statement file<input ref={fileRef} className="field" type="file" accept=".csv,.xlsx" /></label></div>{preview && <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">{fields.map((field) => <label className="block text-sm" key={field.key}>{field.label}{field.required ? ' *' : ''}<select className="field" value={mapping[field.key] ?? ''} onChange={(event) => setMapping((current) => ({ ...current, [field.key]: event.target.value || undefined }))}><option value="">Not mapped</option>{preview.headers.map((header) => <option key={header}>{header}</option>)}</select></label>)}</div>}<label className="block text-sm">Save mapping as template<input className="field" value={templateName} onChange={(event) => setTemplateName(event.target.value)} placeholder="Optional template name" /></label><div className="flex flex-wrap gap-2"><button className="glass-chip" type="button" disabled={busy} onClick={() => send('preview')}>{busy ? 'Working…' : 'Preview and validate'}</button><button className="action-button" type="button" disabled={busy || !preview?.mappingComplete || preview.duplicateFile || !preview.rows.length} onClick={() => send('commit')}>Commit atomically</button></div>{message && <p className={`text-sm ${message.toLowerCase().includes('fail') || message.toLowerCase().includes('error') || message.toLowerCase().includes('duplicate') ? 'text-critical' : 'text-ink-2'}`}>{message}</p>}{preview?.duplicateFile && <p className="text-sm font-bold text-critical">This file hash already exists and cannot be imported again.</p>}{preview?.rows?.length ? <div className="max-h-72 overflow-auto rounded-md border border-rule"><table className="w-full text-xs"><thead className="sticky top-0 bg-paper-2"><tr><th className="px-3 py-2 text-left">Date</th><th className="px-3 py-2 text-left">Description</th><th className="px-3 py-2 text-right">Amount</th><th className="px-3 py-2 text-left">Currency</th></tr></thead><tbody>{preview.rows.slice(0, 50).map((row, index) => <tr className="border-t border-rule" key={String(row.rowNo ?? index)}><td className="px-3 py-2">{String(row.transactionDate)}</td><td className="px-3 py-2">{String(row.description)}</td><td className="px-3 py-2 text-right font-mono">{Number(row.amount).toFixed(2)}</td><td className="px-3 py-2">{String(row.currency)}</td></tr>)}</tbody></table></div> : null}</div> : <p className="mt-4 text-sm text-caution">Create a bank account before importing a statement.</p>}</section>;
}
