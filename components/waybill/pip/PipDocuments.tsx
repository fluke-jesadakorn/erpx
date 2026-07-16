import React from 'react';
import { loadAttachmentsForWaybill } from '@/waybill/queries';
import type { SecondaryLocale } from '@/server/locale';
import { ZoneSection } from '../ZoneSection';
import { AttachmentRow } from '../AttachmentRow';
import { fmtSize } from '../ui';
import { T } from '@/components/i18n/TServer';

interface Props {
  waybillId: string;
  pipKey: string;
  locale?: SecondaryLocale;
}

export async function PipDocuments({ waybillId, pipKey, locale }: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  const all = await loadAttachmentsForWaybill(waybillId);
  const pipAttachments = all.filter((a) => a.stage_key === pipKey);
  if (pipAttachments.length === 0) return null;
  const totalSize = pipAttachments.reduce((s, a) => s + Number(a.byte_size ?? 0), 0);

  return (
    <ZoneSection
      icon={<span aria-hidden>📎</span>}
      label={<T id="waybill.timeline.documents" locale={localeSafe} />}
      count={fmtSize(totalSize)}
      meta={
        pipAttachments.length > 0
          ? <span>{pipAttachments.length} file{pipAttachments.length === 1 ? '' : 's'}</span>
          : undefined
      }
      tone="indigo"
    >
      <div className="divide-y divide-slate-800/40">
        {pipAttachments.map((a) => (
          <AttachmentRow key={a.id} waybillId={waybillId} attachment={a} />
        ))}
      </div>
    </ZoneSection>
  );
}
