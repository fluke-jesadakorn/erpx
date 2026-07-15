import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { PERM } from '@folio-lib/perm';


export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.invocation.view });
  if (guard.response) return guard.response;
  const url = new URL(req.url);
  const limit = Math.min(parseInt(url.searchParams.get('limit') || '50', 10), 500);
  const section = url.searchParams.get('section');
  const params: unknown[] = [];
  let where = '';
  if (section) { params.push(section); where = `WHERE i.section_key = $${params.length}`; }
  params.push(limit);
  const r = await query(
    `SELECT i.id, i.section_key, i.task_type, i.status, i.error,
            i.latency_ms, i.prompt_tokens, i.response_tokens, i.created_at,
            p.name AS provider_name, m.name AS model_name, s.name AS staff_name
     FROM ai_invocations i
     LEFT JOIN ai_providers p ON p.id = i.provider_id
     LEFT JOIN ai_models m ON m.id = i.model_id
     LEFT JOIN ai_staff s ON s.id = i.staff_id
     ${where}
     ORDER BY i.id DESC
     LIMIT $${params.length}`,
    params,
  );
  return NextResponse.json({ invocations: r.rows });
}