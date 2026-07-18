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
    <details className="group rounded-md border border-rule/60 bg-paper-2/50">
      <summary className="cursor-pointer list-none px-4 py-3 [&::-webkit-details-marker]:hidden">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <span aria-hidden className="grid h-9 w-9 place-items-center rounded-md bg-paper-2 text-lg ring-1 ring-rule">
              📜
            </span>
            <div className="flex flex-col">
              <span className="text-base font-bold text-ink">
                <T id="waybill.audit.logTitle" locale={localeSafe} /> ({events.length})
              </span>
              <span className="font-mono text-xs uppercase tracking-widest text-mute">
                <T id="waybill.audit.appendOnly" locale={localeSafe} />
              </span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {integrity.ok ? (
              <span className="inline-flex items-center gap-1.5 rounded-full border border-positive bg-positive px-2.5 py-1 text-xs font-mono font-bold uppercase text-positive-soft">
                <span aria-hidden>✓</span>
                <span>
                  <T id="waybill.audit.hmacVerified" locale={localeSafe} />
                </span>
              </span>
            ) : (
              <span className="inline-flex items-center gap-1.5 rounded-full border border-critical bg-critical px-2.5 py-1 text-xs font-mono font-bold uppercase text-critical-soft">
                <span aria-hidden>⚠</span>
                <span>
                  <T id="waybill.audit.integrityFailed" locale={localeSafe} />
                  {integrity.reason ? `: ${integrity.reason}` : ''}
                </span>
              </span>
            )}
            <span className="font-mono text-xs uppercase tracking-wider text-mute group-open:hidden">
              ▶
            </span>
            <span className="font-mono text-xs uppercase tracking-wider text-mute hidden group-open:inline">
              ▼
            </span>
          </div>
        </div>
      </summary>

      <div className="border-t border-rule/60 px-4 py-4">
<p className="font-mono text-xs uppercase tracking-widest text-mute">
          waybill: <span className="text-info">{waybillId}</span> ·{' '}
          <T id="waybill.audit.eventsTotal" locale={localeSafe} hideSecondary /> {integrity.total}
        </p>
        {events.length === 0 ? (
          <p className="mt-3 text-sm italic text-mute">
            <T id="waybill.audit.noEvents" locale={localeSafe} hideSecondary />
          </p>
        ) : (
          <div className="mt-3 overflow-x-auto">
            <table className="w-full min-w-[640px] border-collapse text-xs">
              <thead>
                <tr className="text-left font-mono text-xs uppercase tracking-widest text-mute">
                  <th className="border-b border-rule px-2 py-1.5">#</th>
                  <th className="border-b border-rule px-2 py-1.5">
                    <T id="waybill.audit.kind" locale={localeSafe} hideSecondary />
                  </th>
                  <th className="border-b border-rule px-2 py-1.5">
                    <T id="waybill.audit.fromTo" locale={localeSafe} hideSecondary />
                  </th>
                  <th className="border-b border-rule px-2 py-1.5">
                    <T id="waybill.audit.actor" locale={localeSafe} hideSecondary />
                  </th>
                  <th className="border-b border-rule px-2 py-1.5">
                    <T id="waybill.audit.at" locale={localeSafe} hideSecondary />
                  </th>
                  <th className="border-b border-rule px-2 py-1.5">
                    <T id="waybill.audit.payload" locale={localeSafe} hideSecondary />
                  </th>
                </tr>
              </thead>
              <tbody>
                {events.map((e) => {
                  const occurredAt = formatDateServer(e.occurred_at, localeSafe);
                  return (
                    <tr key={e.id} className="align-top font-mono text-ink-2">
                      <td className="border-b border-rule/40 px-2 py-2 font-bold text-info">
                        {e.sequence}
                      </td>
                      <td className="border-b border-rule/40 px-2 py-2 font-bold text-ink">
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
                      <td className="border-b border-rule/40 px-2 py-2 text-ink-2">
                        {e.stage_from ?? '—'} → <span className="text-info">{e.stage_to ?? '—'}</span>
                      </td>
                      <td className="border-b border-rule/40 px-2 py-2 text-mute">
                        {e.actor_id != null ? (
                          <>
                            <span className="text-ink-2">
                              {roleDisplay(e.actor_role, localeSafe)}
                            </span>{' '}
                            <span className="text-mute">#{e.actor_id}</span>
                          </>
                        ) : (
                          <span className="text-mute">—</span>
                        )}
                      </td>
                      <td className="border-b border-rule/40 px-2 py-2 text-mute">
                        {occurredAt}
                      </td>
                      <td className="border-b border-rule/40 px-2 py-2">
                        {e.payload ? (
                          <details>
                            <summary className="cursor-pointer text-info hover:text-info-soft">
                              <T id="waybill.audit.expand" locale={localeSafe} hideSecondary />
                            </summary>
                            <pre className="mt-1 max-w-md max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-paper p-2 text-xs text-ink-2">
                              {payloadStringify(e.payload)}
                            </pre>
                          </details>
                        ) : (
                          <span className="text-mute">—</span>
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
