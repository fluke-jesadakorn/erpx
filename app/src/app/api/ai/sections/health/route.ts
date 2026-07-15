import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { SECTION_CATALOG } from '@folio-lib/ai/sections';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { PERM } from '@folio-lib/perm';


const ACTIVE_WINDOW_DAYS = 7;

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.section_health.view });
  if (guard.response) return guard.response;

  const healthRes = await query<{
    section_key: string; task_type: string; assignment_enabled: boolean;
    provider_name: string | null; model_name: string | null;
    ok_calls: string; err_calls: string; total_calls: string;
    last_invocation_at: string | null; first_invocation_at: string | null;
  }>(
    `SELECT section_key, task_type, assignment_enabled,
            provider_name, model_name,
            ok_calls, err_calls, total_calls,
            last_invocation_at, first_invocation_at
     FROM ai_section_health`,
  );

  const catalogKeys = new Set(SECTION_CATALOG.map((c) => c.key));
  const assignedKeys = new Set(healthRes.rows.map((a) => a.section_key));
  const labelByKey = new Map(SECTION_CATALOG.map((c) => [c.key, c]));

  const orphanRes = await query<{
    section_key: string; task_type: string; ok_calls: string; err_calls: string; total_calls: string; last_invocation_at: string | null;
  }>(
    `SELECT section_key, task_type,
            COUNT(*) FILTER (WHERE status='ok')    AS ok_calls,
            COUNT(*) FILTER (WHERE status='error') AS err_calls,
            COUNT(*)                              AS total_calls,
            MAX(created_at) AS last_invocation_at
     FROM ai_invocations
     GROUP BY section_key, task_type
     ORDER BY section_key`,
  );
  const orphans = orphanRes.rows.filter(
    (r) => !assignedKeys.has(r.section_key) && !catalogKeys.has(r.section_key),
  );

  const cutoff = Date.now() - ACTIVE_WINDOW_DAYS * 86400_000;
  const totals = { ACTIVE: 0, DORMANT: 0, NEVER_CALLED: 0, UNCONFIGURED: 0, ORPHAN: 0 };
  const sections: unknown[] = [];

  for (const a of healthRes.rows) {
    const meta = labelByKey.get(a.section_key);
    const total = parseInt(a.total_calls, 10) || 0;
    const lastMs = a.last_invocation_at ? new Date(a.last_invocation_at).getTime() : 0;
    const bucket = total === 0 ? 'NEVER_CALLED' : lastMs >= cutoff ? 'ACTIVE' : 'DORMANT';
    totals[bucket]++;
    sections.push({
      bucket, section_key: a.section_key,
      label: meta?.label || a.section_key, labelTh: meta?.labelTh || a.section_key,
      task: a.task_type, provider_name: a.provider_name, model_name: a.model_name,
      ok_calls: parseInt(a.ok_calls, 10) || 0, err_calls: parseInt(a.err_calls, 10) || 0, total_calls: total,
      last_invocation_at: a.last_invocation_at, first_invocation_at: a.first_invocation_at,
      assignment_enabled: a.assignment_enabled,
    });
  }

  for (const c of SECTION_CATALOG) {
    if (assignedKeys.has(c.key)) continue;
    totals.UNCONFIGURED++;
    sections.push({
      bucket: 'UNCONFIGURED', section_key: c.key,
      label: c.label, labelTh: c.labelTh, task: c.task,
      provider_name: null, model_name: null,
      ok_calls: 0, err_calls: 0, total_calls: 0,
      last_invocation_at: null, first_invocation_at: null, assignment_enabled: null,
    });
  }

  for (const o of orphans) {
    totals.ORPHAN++;
    sections.push({
      bucket: 'ORPHAN', section_key: o.section_key,
      label: o.section_key, labelTh: '(ไม่อยู่ในแคตตาล็อก)', task: o.task_type,
      provider_name: null, model_name: null,
      ok_calls: parseInt(o.ok_calls, 10) || 0, err_calls: parseInt(o.err_calls, 10) || 0, total_calls: parseInt(o.total_calls, 10) || 0,
      last_invocation_at: o.last_invocation_at, first_invocation_at: null, assignment_enabled: null,
    });
  }

  const coverage_pct = SECTION_CATALOG.length > 0
    ? Math.round((totals.ACTIVE / SECTION_CATALOG.length) * 1000) / 10
    : 0;

  return NextResponse.json({
    generated_at: new Date().toISOString(),
    active_window_days: ACTIVE_WINDOW_DAYS,
    catalog_total: SECTION_CATALOG.length,
    totals,
    coverage_pct,
    sections,
  });
}