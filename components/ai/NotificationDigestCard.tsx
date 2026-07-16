import 'server-only';
import { generateDigest, loadLatestDigest, type NotificationDigest } from '@/ai/digest';
import { loadActor } from '@/server/guard';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

const SEVERITY_BG: Record<string, string> = {
  info: 'border-slate-500/40 bg-slate-500/10 text-slate-300',
  success: 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300',
  warning: 'border-amber-500/40 bg-amber-500/10 text-amber-300',
  error: 'border-rose-500/40 bg-rose-500/10 text-rose-300',
};

export async function NotificationDigestCard() {
  const actor = await loadActor();
  if (!actor) return null;

  let digest: NotificationDigest | null = await loadLatestDigest(actor.id);
  if (!digest) {
    digest = await generateDigest(actor.id, { lang: 'en' });
  }
  if (!digest) return null;
  const locale = await getSecondaryLocale();

  const sevCls = SEVERITY_BG[digest.severity] ?? SEVERITY_BG.info;
  const bullets = Array.isArray(digest.bullets) ? digest.bullets : [];

  return (
    <div className="hidden lg:flex flex-col gap-1 rounded-lg border border-glass-border bg-surface-glass-heavy px-3 py-1.5 max-w-[300px]">
      <div className="flex items-center gap-2">
        <span className="text-[10px] font-mono uppercase tracking-widest text-slate-400">
          <T id="ai.notification.digestTitle" locale={locale} />
        </span>
        <span className={`rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wide ${sevCls}`}>{digest.severity}</span>
        <span className="ml-auto text-[10px] font-mono text-slate-500">
          · <T id="ai.notification.eventsCount" values={{ n: digest.sourceCount }} locale={locale} />
        </span>
      </div>
      {bullets.length > 0 ? (
        <ul className="space-y-0.5 text-xs text-slate-200">
          {bullets.slice(0, 3).map((b, i) => (
            <li key={i} className="flex gap-1.5 truncate">
              <span className="text-slate-500">•</span>
              <span className="truncate">{b}</span>
            </li>
          ))}
        </ul>
      ) : (
        <div className="text-xs text-slate-500">
          <T id="ai.notification.empty" locale={locale} />
        </div>
      )}
    </div>
  );
}