'use client';

import React, { useState } from 'react';
import type { WaybillAttachmentRow } from '@/waybill/attachments';
import { WAYBILL_KINDS, type WaybillAttachmentKind } from '@/waybill/kinds';

interface Props {
  waybillId: string;
  attachments: WaybillAttachmentRow[];
  defaultSections?: { cover?: boolean; rail?: boolean; audit?: boolean; attachments?: boolean };
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

export function ExportPdfButton({ waybillId, attachments, defaultSections }: Props) {
  const [open, setOpen] = useState(false);
  const [pageSize, setPageSize] = useState<'A4' | 'Letter'>('Letter');
  const [sections, setSections] = useState({
    cover: defaultSections?.cover ?? true,
    rail: defaultSections?.rail ?? true,
    audit: defaultSections?.audit ?? true,
    attachments: defaultSections?.attachments ?? true,
  });
  const [includedIds, setIncludedIds] = useState<Set<string>>(
    () => new Set(attachments.map((a) => String(a.id))),
  );

  function toggle(id: string): void {
    setIncludedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function download(): void {
    let url = `/api/waybill/${waybillId}/attachments/file?key=`;
    const params: string[] = [];
    if (!sections.cover) params.push('section=cover:0');
    if (!sections.rail) params.push('section=rail:0');
    if (!sections.audit) params.push('section=audit:0');
    if (!sections.attachments) params.push('section=attachments:0');
    if (pageSize === 'A4') params.push('size=A4');
    if (sections.attachments && includedIds.size > 0 && includedIds.size < attachments.length) {
      params.push('attachment_ids=' + Array.from(includedIds).join(','));
    }
    url += params.join('&');
    window.location.href = url;
    setOpen(false);
  }

  const kindMeta = (k: string) =>
    WAYBILL_KINDS[k as WaybillAttachmentKind] ?? WAYBILL_KINDS.other;

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="rounded-lg border border-info bg-info px-3 py-1.5 text-xs font-bold text-info-soft hover:bg-info"
        title={`Export PDF — ${attachments.length} attachments`}
      >
        ⤓ Export PDF
      </button>

      {open && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Export Waybill PDF"
          className="fixed inset-0 z-modal flex items-center justify-center bg-paper-2/70 backdrop-blur-sm"
        >
          <div className="w-full max-w-2xl rounded-md border border-rule bg-paper p-5 shadow-2xl">
            <header className="flex items-start justify-between">
              <div>
                <h2 className="text-sm font-bold text-ink">
                  Export Waybill PDF
                </h2>
                <p className="mt-0.5 font-mono text-xs text-mute">
                  {waybillId}_combined.pdf
                </p>
              </div>
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="rounded border border-rule px-2 py-0.5 text-xs text-ink-2 hover:border-critical hover:text-critical-soft"
                aria-label="Close"
              >
                ✕
              </button>
            </header>

            <div className="mt-4 grid gap-4 sm:grid-cols-2">
              <div>
                <div className="text-xs font-mono uppercase tracking-widest text-mute">
                  Sections
                </div>
                <div className="mt-2 space-y-1.5 text-xs text-ink">
                  {(['cover','rail','audit','attachments'] as const).map((k) => (
                    <label key={k} className="flex items-center gap-2">
                      <input
                        type="checkbox"
                        checked={sections[k]}
                        onChange={(e) => setSections((prev) => ({ ...prev, [k]: e.target.checked }))}
                      />
                      <span>{k}</span>
                    </label>
                  ))}
                </div>

                <div className="mt-3">
                  <div className="text-xs font-mono uppercase tracking-widest text-mute">
                    Page size
                  </div>
                  <select
                    value={pageSize}
                    onChange={(e) => setPageSize(e.target.value as 'A4' | 'Letter')}
                    className="mt-1 rounded border border-rule bg-paper px-2 py-1 text-xs text-ink"
                  >
                    <option value="Letter">US Letter</option>
                    <option value="A4">A4</option>
                  </select>
                </div>
              </div>

              <div>
                <div className="text-xs font-mono uppercase tracking-widest text-mute">
                  Attachments ({includedIds.size}/{attachments.length})
                </div>
                {attachments.length === 0 && (
                  <p className="mt-2 text-xs italic text-mute">No attachments.</p>
                )}
                <ul className="mt-2 max-h-64 space-y-1.5 overflow-y-auto pr-2">
                  {attachments.map((a) => (
                    <li key={a.id}>
                      <label className="flex items-start gap-2 rounded border border-rule bg-paper-2/50 p-2 text-sm hover:border-info">
                        <input
                          type="checkbox"
                          checked={includedIds.has(String(a.id))}
                          onChange={() => toggle(String(a.id))}
                          className="mt-0.5"
                        />
                        <span className="flex-1">
                          <span className="block">
                            <span aria-hidden>{kindMeta(a.kind).emoji}</span>{' '}
                            <span className="font-mono text-info-soft">{a.filename}</span>
                          </span>
                          <span className="block font-mono text-xs text-mute">
                            {a.kind} · {fmtSize(a.byte_size)} · stage: {a.stage_key}
                          </span>
                        </span>
                      </label>
                    </li>
                  ))}
                </ul>
              </div>
            </div>

            <footer className="mt-5 flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="rounded-lg border border-rule px-3 py-1.5 text-xs text-ink-2"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={download}
                className="rounded-lg bg-info px-3 py-1.5 text-xs font-bold text-ink hover:bg-info"
              >
                ⤓ Generate & Download
              </button>
            </footer>
          </div>
        </div>
      )}
    </>
  );
}
