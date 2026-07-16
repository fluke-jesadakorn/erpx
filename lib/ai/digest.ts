import 'server-only';

import { query } from '@folio-lib/db';
import { aiInvoke } from '@folio-lib/ai/router';
import { listRecentNotifications } from '@folio-lib/notifications/queries';

export type DigestSeverity = 'info' | 'success' | 'warning' | 'error';

export interface NotificationDigest {
  bullets: string[];
  severity: DigestSeverity;
  generatedAt: string;
  sourceCount: number;
}

interface DigestRow {
  id: string;
  user_id: number;
  period_start: string;
  period_end: string;
  severity: DigestSeverity;
  bullets: string[];
  source_count: number;
  generated_at: string;
}

const SEVERITY_ORDER: Record<DigestSeverity, number> = {
  info: 0,
  success: 1,
  warning: 2,
  error: 3,
};

function langLine(lang: 'en' | 'th' | 'de'): string {
  if (lang === 'th') return 'ตอบเป็นภาษาไทย';
  if (lang === 'de') return 'Antworten Sie auf Deutsch';
  return 'Reply in English';
}

export async function generateDigest(
  _actorId: number,
  opts?: { limit?: number; lang?: 'en' | 'th' | 'de' },
): Promise<NotificationDigest | null> {
  const limit = opts?.limit ?? 50;
  const lang = opts?.lang ?? 'en';
  const events = await listRecentNotifications(limit);
  if (events.length === 0) return null;

  let highest: DigestSeverity = 'info';
  for (const e of events) {
    const sev = (e.severity ?? 'info') as DigestSeverity;
    if (SEVERITY_ORDER[sev] > SEVERITY_ORDER[highest]) highest = sev;
  }

  const r = await aiInvoke('notification:digest', 'chat', {
    systemPrompt: `You condense a stream of internal-business notifications into a tight 3-bullet digest for an executive. Be specific with numbers and names. Never invent details. If there are no actionable items, say "No action needed." Output as JSON only: {"bullets":["...","...","..."]}. ${langLine(lang)}`,
    text: events.map((e) => `- [${e.severity}] ${e.type}: ${e.message}`).join('\n'),
    temperature: 0.2,
    maxTokens: 600,
  });
  if (!r.ok || !r.text) return null;

  let bullets: string[] = [];
  try {
    const match = r.text.match(/\{[\s\S]*\}/);
    const parsed = JSON.parse(match?.[0] ?? '{}');
    if (Array.isArray(parsed.bullets)) {
      bullets = parsed.bullets.slice(0, 3).map((b: unknown) => String(b));
    }
  } catch {
    /* keep empty */
  }

  return {
    bullets,
    severity: highest,
    generatedAt: new Date().toISOString(),
    sourceCount: events.length,
  };
}

export async function saveDigest(
  userId: number,
  periodStart: Date,
  periodEnd: Date,
  digest: NotificationDigest,
): Promise<void> {
  await query(
    `INSERT INTO notification_digests
       (user_id, period_start, period_end, severity, bullets, source_count, generated_at)
     VALUES ($1, $2, $3, $4, $5::jsonb, $6, now())
     ON CONFLICT (user_id, period_start, period_end) DO UPDATE
       SET severity = EXCLUDED.severity,
           bullets = EXCLUDED.bullets,
           source_count = EXCLUDED.source_count,
           generated_at = now()`,
    [
      userId,
      periodStart.toISOString(),
      periodEnd.toISOString(),
      digest.severity,
      JSON.stringify(digest.bullets),
      digest.sourceCount,
    ],
  );
}

export async function loadLatestDigest(userId: number): Promise<NotificationDigest | null> {
  const r = await query<DigestRow>(
    `SELECT id::text AS id, user_id,
            period_start::text AS period_start,
            period_end::text AS period_end,
            severity, bullets, source_count,
            generated_at::text AS generated_at
       FROM notification_digests
      WHERE user_id = $1
      ORDER BY generated_at DESC
      LIMIT 1`,
    [userId],
  );
  const row = r.rows[0];
  if (!row) return null;
  return {
    bullets: Array.isArray(row.bullets) ? row.bullets.map((b: unknown) => String(b)) : [],
    severity: row.severity,
    generatedAt: row.generated_at,
    sourceCount: row.source_count,
  };
}