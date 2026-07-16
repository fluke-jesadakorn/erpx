import 'server-only';
import { getCustomerAdvisory } from '@/customer/queries';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

const SEVERITY_STYLE: Record<string, string> = {
  ok: 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300',
  watch: 'border-amber-500/40 bg-amber-500/10 text-amber-300',
  critical: 'border-rose-500/40 bg-rose-500/10 text-rose-300',
};

const SEVERITY_LABEL: Record<string, string> = {
  ok: 'OK',
  watch: 'Watch',
  critical: 'Critical',
};

export async function CustomerAdvisoryCard({ customerId, lang = 'en' }: { customerId: number; lang?: 'en' | 'th' | 'de' }) {
  void lang;
  const locale = await getSecondaryLocale();
  let advisory = null;
  try {
    advisory = await getCustomerAdvisory(customerId, { lang });
  } catch { /* ignore */ }
  if (!advisory) return null;
  const sev = advisory.severity ?? 'ok';
  const sevCls = SEVERITY_STYLE[sev] ?? SEVERITY_STYLE.ok;
  const sevLabel = SEVERITY_LABEL[sev] ?? 'OK';

  return (
    <section className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
      <div className="mb-2 flex items-center gap-2">
        <h3 className="text-xs font-mono uppercase tracking-widest text-slate-400">
          <T id="customers.detailAdvisory" locale={locale} />
        </h3>
        <span className={`rounded-full border px-2 py-0.5 text-xs font-mono ${sevCls}`}>{sevLabel}</span>
        <span className="ml-auto text-xs font-mono text-slate-500">
          {new Date(advisory.computedAt).toISOString().slice(0, 16).replace('T', ' ')}
        </span>
      </div>
      <p className="whitespace-pre-wrap text-sm leading-relaxed text-slate-200">{advisory.advisory}</p>
    </section>
  );
}