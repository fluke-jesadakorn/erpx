import React from 'react';
import type { WaybillEventRow } from '@/waybill/events';
import type { SecondaryLocale } from '@/server/locale';
import {
  eventKindLabel,
  payloadStringify,
  roleDisplay,
} from './ui';
import { EventExplainButton } from './EventExplainButton';
import { formatDateServer } from '@/components/i18n/formattersServer';
import { T } from '@/components/i18n/TServer';

interface Props {
  waybillId: string;
  events: WaybillEventRow[];
  integrity: { ok: boolean; reason?: string; total: number };
  locale?: SecondaryLocale;
}

export async function WaybillAuditSection({
  waybillId,
  events,
  integrity,
  locale,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';

  return (
    <details className="group rounded-2xl border border-slate-800/60 bg-slate-950/40">
      <summary className="cursor-pointer list-none px-4 py-3 [&::-webkit-details-marker]:hidden">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <span aria-hidden className="grid h-9 w-9 place-items-center rounded-xl bg-gradient-to-br from-slate-700/50 to-slate-800/50 text-lg ring-1 ring-slate-700">
              📜
            </span>
            <div className="flex flex-col">
              <span className="text-base font-bold text-white">
                <T id="waybill.audit.logTitle" locale={localeSafe} /> ({events.length})
              </span>
              <span className="font-mono text-xs uppercase tracking-widest text-slate-500">
                <T id="waybill.audit.appendOnly" locale={localeSafe} />
              </span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {integrity.ok ? (
              <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2.5 py-1 text-xs font-mono font-bold uppercase text-emerald-200">
                <span aria-hidden>✓</span>
                <span>
                  <T id="waybill.audit.hmacVerified" locale={localeSafe} />
                </span>
              </span>
            ) : (
              <span className="inline-flex items-center gap-1.5 rounded-full border border-rose-500/50 bg-rose-500/15 px-2.5 py-1 text-xs font-mono font-bold uppercase text-rose-200">
                <span aria-hidden>⚠</span>
                <span>
                  <T id="waybill.audit.integrityFailed" locale={localeSafe} />
                  {integrity.reason ? `: ${integrity.reason}` : ''}
                </span>
              </span>
            )}
            <span className="font-mono text-xs uppercase tracking-wider text-slate-500 group-open:hidden">
              ▶
            </span>
            <span className="font-mono text-xs uppercase tracking-wider text-slate-500 hidden group-open:inline">
              ▼
            </span>
          </div>
        </div>
      </summary>

      <div className="border-t border-slate-800/60 px-4 py-4">
        <p className="font-mono text-xs uppercase tracking-widest text-slate-500">
          waybill: <span className="text-cyan-300">{waybillId}</span> ·{' '}
          <T id="waybill.audit.eventsTotal" locale={localeSafe} /> {integrity.total}
        </p>
        {events.length === 0 ? (
          <p className="mt-3 text-sm italic text-slate-500">
            <T id="waybill.audit.noEvents" locale={localeSafe} />
          </p>
        ) : (
          <div className="mt-3 overflow-x-auto">
            <table className="w-full min-w-[640px] border-collapse text-xs">
              <thead>
                <tr className="text-left font-mono text-xs uppercase tracking-widest text-slate-500">
                  <th className="border-b border-slate-800/60 px-2 py-1.5">#</th>
                  <th className="border-b border-slate-800/60 px-2 py-1.5">
                    <T id="waybill.audit.kind" locale={localeSafe} />
                  </th>
                  <th className="border-b border-slate-800/60 px-2 py-1.5">
                    <T id="waybill.audit.fromTo" locale={localeSafe} />
                  </th>
                  <th className="border-b border-slate-800/60 px-2 py-1.5">
                    <T id="waybill.audit.actor" locale={localeSafe} />
                  </th>
                  <th className="border-b border-slate-800/60 px-2 py-1.5">
                    <T id="waybill.audit.at" locale={localeSafe} />
                  </th>
                  <th className="border-b border-slate-800/60 px-2 py-1.5">
                    <T id="waybill.audit.payload" locale={localeSafe} />
                  </th>
                </tr>
              </thead>
              <tbody>
                {events.map((e) => {
                  const occurredAt = formatDateServer(e.occurred_at, localeSafe);
                  return (
                    <tr key={e.id} className="align-top font-mono text-slate-300">
                      <td className="border-b border-slate-800/40 px-2 py-2 font-bold text-cyan-300">
                        {e.sequence}
                      </td>
                      <td className="border-b border-slate-800/40 px-2 py-2 font-bold text-white">
                        <div className="flex items-center gap-2">
                          <span>{eventKindLabel(e.kind, localeSafe)}</span>
                          <EventExplainButton
                            waybillId={waybillId}
                            eventId={String(e.id)}
                            eventKind={e.kind}
                            fromStage={e.stage_from ?? ''}
                            toStage={e.stage_to ?? ''}
                            actorName={e.actor_id != null ? `${roleDisplay(e.actor_role, localeSafe)} #${e.actor_id}` : undefined}
                          />
                        </div>
                      </td>
                      <td className="border-b border-slate-800/40 px-2 py-2 text-slate-400">
                        {e.stage_from ?? '—'} → <span className="text-cyan-300">{e.stage_to ?? '—'}</span>
                      </td>
                      <td className="border-b border-slate-800/40 px-2 py-2 text-slate-500">
                        {e.actor_id != null ? (
                          <>
                            <span className="text-slate-300">
                              {roleDisplay(e.actor_role, localeSafe)}
                            </span>{' '}
                            <span className="text-slate-500">#{e.actor_id}</span>
                          </>
                        ) : (
                          <span className="text-slate-600">—</span>
                        )}
                      </td>
                      <td className="border-b border-slate-800/40 px-2 py-2 text-slate-500">
                        {occurredAt}
                      </td>
                      <td className="border-b border-slate-800/40 px-2 py-2">
                        {e.payload ? (
                          <details>
                            <summary className="cursor-pointer text-cyan-300 hover:text-cyan-200">
                              <T id="waybill.audit.expand" locale={localeSafe} />
                            </summary>
                            <pre className="mt-1 max-w-md overflow-x-auto whitespace-pre-wrap break-all rounded bg-slate-950 p-2 text-xs text-slate-300">
                              {payloadStringify(e.payload)}
                            </pre>
                          </details>
                        ) : (
                          <span className="text-slate-600">—</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </details>
  );
}
