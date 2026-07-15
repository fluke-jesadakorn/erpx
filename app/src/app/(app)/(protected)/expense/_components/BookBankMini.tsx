import React from 'react';

interface BookBankMiniProps {
  slips: Array<{
    file_path: string;
    mime_type: string;
    file_size: number;
    kind: string | null;
    status: string | null;
    bank_name: string | null;
    bank_branch: string | null;
    account_number: string | null;
    account_name: string | null;
  }>;
  waybillId: string;
  currentStage: string;
}

export async function BookBankMini({ slips }: BookBankMiniProps) {
  const slip = slips.find((s) => s.kind === 'book-bank' || s.kind === 'book_bank') ?? null;
  if (!slip) {
    return (
      <span className="text-sm font-mono text-slate-600 italic">
        no book bank
      </span>
    );
  }

  const statusOk = slip.status === 'confirmed' || slip.status === 'verified' || slip.status === 'approved';
  const statusColor = statusOk
    ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-200'
    : 'border-amber-500/30 bg-amber-500/10 text-amber-300';

  return (
    <div className="inline-flex items-center gap-2 rounded-lg border border-slate-800/60 bg-slate-950/40 px-2.5 py-1.5">
      <span aria-hidden className="text-sm leading-none">🏦</span>
      <span className="truncate font-mono text-sm text-slate-300">
        {slip.bank_name && <span className="text-slate-100">{slip.bank_name}</span>}
        {slip.bank_name && slip.account_number && <span className="text-slate-600 mx-0.5">·</span>}
        {slip.account_number && <span className="text-cyan-300 font-bold">#{slip.account_number}</span>}
        {(slip.bank_name || slip.account_number) && slip.account_name && <span className="text-slate-600 mx-0.5">·</span>}
        {slip.account_name && <span className="text-slate-300">{slip.account_name}</span>}
      </span>
      <span className={`rounded-full border px-1.5 py-0.5 text-xs font-mono uppercase tracking-widest ${statusColor}`}>
        {statusOk ? '✓ confirmed' : slip.status ?? 'pending'}
      </span>
    </div>
  );
}
