import React from 'react';
import { loadAttachmentsForWaybill } from '@folio-lib/waybill/queries';
import { WAYBILL_KINDS, type WaybillAttachmentKind } from '@folio-lib/waybill/kinds';
import { fmtSize } from '@/components/waybill/ui';

interface DocListProps {
  waybillId: string;
  currentStage: string;
}

export async function DocList({ waybillId }: DocListProps) {
  const all = await loadAttachmentsForWaybill(waybillId);
  if (all.length === 0) return null;

  return (
    <div className="space-y-1">
      {all.map((a) => {
        const meta = WAYBILL_KINDS[a.kind as WaybillAttachmentKind] ?? WAYBILL_KINDS.other;
        return (
          <div
            key={a.id}
            className="flex items-center gap-2 rounded-lg border border-slate-800/60 bg-slate-950/40 px-2.5 py-1.5"
          >
            <span aria-hidden className="text-sm leading-none">{meta.emoji}</span>
            <span className="min-w-0 flex-1 truncate font-mono text-sm text-slate-300">
              {a.filename}
            </span>
            <span className="shrink-0 font-mono text-xs text-slate-600">
              {fmtSize(a.byte_size)}
            </span>
          </div>
        );
      })}
    </div>
  );
}
