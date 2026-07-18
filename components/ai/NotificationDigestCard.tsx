import 'server-only';
import { generateDigest, loadLatestDigest, type NotificationDigest } from '@/ai/digest';
import { loadActor } from '@/server/guard';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

const SEVERITY_BG: Record<string, string> = {
  info: 'border-rule/40 bg-paper-2/10 text-ink-2',
  success: 'border-positive bg-positive text-positive',
  warning: 'border-caution bg-caution text-caution',
  error: 'border-critical bg-critical text-critical',
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
    <div className="hidden lg:flex flex-col gap-1 rounded-lg border border-rule bg-paper-2 px-3 py-1.5 max-w-[300px]">
      <div className="flex items-center gap-2">
        <span className="text-[10px] font-mono uppercase tracking-widest text-ink-2">
          <T id="ai.notification.digestTitle" locale={locale} />
        </span>
        <span className={`rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wide ${sevCls}`}>{digest.severity}</span>
        <span className="ml-auto text-[10px] font-mono text-mute">
          · <T id="ai.notification.eventsCount" values={{ n: digest.sourceCount }} locale={locale} />
        </span>
      </div>
      {bullets.length > 0 ? (
        <ul className="space-y-0.5 text-xs text-ink">
          {bullets.slice(0, 3).map((b, i) => (
            <li key={i} className="flex gap-1.5 truncate">
              <span className="text-mute">•</span>
              <span className="truncate">{b}</span>
            </li>
          ))}
        </ul>
      ) : (
        <div className="text-xs text-mute">
          <T id="ai.notification.empty" locale={locale} />
        </div>
      )}
    </div>
  );
}