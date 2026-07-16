import React from 'react';
import Link from 'next/link';
import {
  Receipt,
  Landmark,
  Paperclip,
  CircleCheck,
  CircleDashed,
  Plus,
} from 'lucide-react';
import { loadAttachmentsForWaybill } from '@folio-lib/waybill/queries';
import { Bilingual } from '@/components/i18n/Bilingual';

interface SlipRow {
  file_path: string;
  mime_type: string;
  file_size: number;
  kind: string | null;
  status: string | null;
  bank_name: string | null;
  bank_branch: string | null;
  account_number: string | null;
  account_name: string | null;
}

interface ReceiptBankCardProps {
  waybillId: string;
  vendorName: string | null;
  totalAmount: string | null;
  currency: string;
  slips: SlipRow[];
}

function fmtTHB(amount: string | null): string {
  if (!amount) return '—';
  const n = parseFloat(amount);
  if (Number.isNaN(n)) return '—';
  return n.toLocaleString('th-TH', { minimumFractionDigits: 2 });
}

function RailHeader({
  Icon,
  en,
  th,
  status,
}: {
  Icon: React.ElementType;
  en: string;
  th: string;
  status: string | null;
}) {
  const ok = status === 'confirmed' || status === 'verified' || status === 'approved';
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="inline-flex items-center gap-1.5 text-[11px] font-mono font-bold text-ink-2 uppercase tracking-widest">
        <Icon className="size-3.5" aria-hidden strokeWidth={2} />
        <Bilingual showSecondary={false} en={en} th={th} />
      </span>
      {status ? (
        <span
          className={
            'inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-widest ' +
            (ok
              ? 'border-positive/40 bg-positive-soft text-positive-strong'
              : 'border-caution/40 bg-caution-soft text-caution-strong')
          }
        >
          {ok ? (
            <CircleCheck className="size-2.5" aria-hidden strokeWidth={2.5} />
          ) : (
            <CircleDashed className="size-2.5" aria-hidden strokeWidth={2} />
          )}
          {ok ? 'ok' : status}
        </span>
      ) : (
        <span className="inline-flex items-center gap-1 rounded-full border border-rule bg-paper-3 px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-widest text-mute">
          <CircleDashed className="size-2.5" aria-hidden strokeWidth={2} />
          <Bilingual showSecondary={false} en="none" th="ไม่มี" />
        </span>
      )}
    </div>
  );
}

export async function ReceiptBankCard({
  waybillId,
  vendorName,
  totalAmount,
  currency,
  slips,
}: ReceiptBankCardProps) {
  const receipt = slips.find((s) => s.kind === 'receipt') ?? null;
  const bank =
    slips.find((s) => s.kind === 'book_bank' || s.kind === 'book-bank') ?? null;
  const attachments = await loadAttachmentsForWaybill(waybillId);
  const fileCount = attachments.length;
  void currency;
  void receipt;

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <div className="rounded-xl border border-rule bg-paper-3/55 px-3.5 py-3 space-y-1.5">
        <RailHeader Icon={Receipt} en="Receipt" th="ใบเสร็จ" status={null} />
        <p className="text-sm font-semibold text-ink truncate">
          {vendorName ?? <span className="text-mute italic font-normal">—</span>}
        </p>
        <div className="flex items-center gap-2 flex-wrap text-xs">
          <span className="font-mono tabular-nums text-info font-semibold text-sm">
            ฿{fmtTHB(totalAmount)}
          </span>
          <span className="text-mute">·</span>
          <span className="inline-flex items-center gap-1 text-mute">
            <Paperclip className="size-3" aria-hidden strokeWidth={2} />
            {fileCount} {fileCount === 1 ? 'file' : 'files'}
          </span>
        </div>
      </div>

      <div className="rounded-xl border border-rule bg-paper-3/55 px-3.5 py-3 space-y-1.5">
        <RailHeader
          Icon={Landmark}
          en="Book bank"
          th="สมุดบัญชี"
          status={bank?.status ?? null}
        />
        {bank ? (
          <>
            <p className="text-sm font-semibold text-ink truncate">
              {bank.bank_name ?? (
                <span className="text-mute italic font-normal">—</span>
              )}
              {bank.bank_branch && (
                <span className="text-mute font-normal"> · {bank.bank_branch}</span>
              )}
            </p>
            <div className="flex items-center gap-2 flex-wrap text-xs">
              {bank.account_number && (
                <span className="font-mono tabular-nums text-info font-semibold text-sm">
                  #{bank.account_number}
                </span>
              )}
              {bank.account_name && (
                <>
                  <span className="text-mute">·</span>
                  <span className="text-ink-2 truncate max-w-[12rem]">
                    {bank.account_name}
                  </span>
                </>
              )}
            </div>
          </>
        ) : (
          <Link
            href={`/waybill/${waybillId}`}
            className="inline-flex items-center gap-1.5 text-xs font-mono text-mute hover:text-accent transition-colors"
          >
            <Plus className="size-3" aria-hidden strokeWidth={2} />
            <Bilingual
              showSecondary={false}
              en="attach slip"
              th="แนบสลิป"
            />
            <span className="text-mute">·</span>
            <span className="text-mute text-[10px]">
              <Bilingual
                showSecondary={false}
                en="only for transfer"
                th="เฉพาะโอนเงิน"
              />
            </span>
          </Link>
        )}
      </div>
    </div>
  );
}