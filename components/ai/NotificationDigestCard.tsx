import 'server-only';
import { generateDigest, loadLatestDigest, type NotificationDigest } from '@/ai/digest';
import { loadActor } from '@/server/guard';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { notificationSchemaReady } from '@/notifications/queries';

const SEVERITY_BG: Record<string, string> = {
  info: 'border-rule/40 bg-paper-2/10 text-ink-2',
  success: 'border-positive bg-positive text-positive',
  warning: 'border-caution bg-caution text-caution',
  error: 'border-critical bg-critical text-critical',
};

export async function NotificationDigestCard() {
  const actor = await loadActor();
  if (!actor) return null;
  if (!(await notificationSchemaReady())) return null;

  let digest: NotificationDigest | null = null;
  try {
    digest = await loadLatestDigest(actor.id);
    if (!digest) digest = await generateDigest(actor.id, { lang: 'en' });
  } catch {}
  if (!digest) return null;
  const locale = await getSecondaryLocale();

  const sevCls = SEVERITY_BG[digest.severity] ?? SEVERITY_BG.info;
  const bullets = Array.isArray(digest.bullets) ? digest.bullets : [];
  const summary = bullets[0] ?? null;

  return (
    <div
      className="hidden h-9 min-w-0 max-w-[300px] items-center gap-2 overflow-hidden rounded-lg border border-rule bg-paper-2 px-3 lg:flex"
      title={bullets.length > 1 ? bullets.join(' · ') : summary ?? undefined}
    >
      <span className="shrink-0 text-[10px] font-mono uppercase tracking-widest text-ink-2">
        <T id="ai.notification.digestTitle" locale={locale} />
      </span>
      <span className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wide ${sevCls}`}>
        {digest.severity}
      </span>
      <span className="min-w-0 truncate text-xs text-ink">
        {summary ?? <T id="ai.notification.empty" locale={locale} />}
      </span>
      <span className="shrink-0 text-[10px] font-mono text-mute">
        · <T id="ai.notification.eventsCount" values={{ n: digest.sourceCount }} locale={locale} />
      </span>
    </div>
  );
}
