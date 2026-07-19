import 'server-only';
import Link from 'next/link';
import { generateDigest, loadLatestDigest, type NotificationDigest } from '@/ai/digest';
import { loadActor } from '@/server/guard';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { notificationSchemaReady } from '@/notifications/queries';

const SEVERITY_CLS: Record<string, { dot: string; pill: string }> = {
  info:    { dot: 'bg-info',    pill: 'border-info/40 bg-info-soft/60 text-info' },
  success: { dot: 'bg-positive', pill: 'border-positive/40 bg-positive-soft/60 text-positive' },
  warning: { dot: 'bg-caution', pill: 'border-caution/40 bg-caution-soft/60 text-caution' },
  error:   { dot: 'bg-critical', pill: 'border-critical/40 bg-critical-soft/60 text-critical' },
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

  const cls = SEVERITY_CLS[digest.severity] ?? SEVERITY_CLS.info;
  const bullets = Array.isArray(digest.bullets) ? digest.bullets : [];
  const summary = bullets[0] ?? null;
  const urgent = digest.severity === 'warning' || digest.severity === 'error';

  return (
    <Link
      href="/inbox"
      title={bullets.length > 1 ? bullets.join(' · ') : summary ?? undefined}
      className="hidden h-9 min-w-0 max-w-[320px] items-center gap-2 overflow-hidden rounded-lg border border-rule bg-paper-2 px-2.5 text-left transition-all hover:border-rule-strong hover:bg-paper-3 lg:flex"
    >
      <span className="shrink-0 text-[10px] font-mono uppercase tracking-[0.14em] text-mute">
        <T id="ai.notification.digestTitle" locale={locale} />
      </span>
      <span className={`inline-flex shrink-0 items-center gap-1 rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wide ${cls.pill}`}>
        <span className={`relative h-1.5 w-1.5 rounded-full ${cls.dot}`}>
          {urgent && (
            <span className={`absolute inset-0 animate-ping rounded-full ${cls.dot} opacity-60`} />
          )}
        </span>
        {digest.severity}
      </span>
      <span className="min-w-0 truncate text-xs text-ink">
        {summary ?? <T id="ai.notification.empty" locale={locale} />}
      </span>
      <span className="shrink-0 text-[10px] font-mono text-mute">
        · <T id="ai.notification.eventsCount" values={{ n: digest.sourceCount }} locale={locale} />
      </span>
    </Link>
  );
}
