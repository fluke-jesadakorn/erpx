import 'server-only';
import { getDashboardForRole } from '@/dashboard/queries';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export async function CockpitAiNarrative({ actorId }: { actorId: number }) {
  const dash = await getDashboardForRole(actorId);
  const ai = (dash as any)?.summary?.aiNarrative ?? null;
  if (!ai) return null;
  const cfo = typeof ai.cfo === 'string' ? ai.cfo.trim() : '';
  const ceo = typeof ai.ceo === 'string' ? ai.ceo.trim() : '';
  if (!cfo && !ceo) return null;
  const locale = await getSecondaryLocale();

  return (
    <section className="rounded-md border border-accent bg-accent-soft p-5">
      <h3 className="mb-3 text-xs font-mono uppercase tracking-widest text-accent">
        <T id="cockpit.narrative" locale={locale} />
      </h3>
      {cfo && (
        <div className="mb-4">
          <div className="mb-1 text-xs font-mono text-mute">CFO</div>
          <p className="whitespace-pre-wrap text-sm leading-relaxed text-ink">{cfo}</p>
        </div>
      )}
      {ceo && (
        <div>
          <div className="mb-1 text-xs font-mono text-mute">CEO</div>
          <p className="whitespace-pre-wrap text-sm leading-relaxed text-ink">{ceo}</p>
        </div>
      )}
    </section>
  );
}