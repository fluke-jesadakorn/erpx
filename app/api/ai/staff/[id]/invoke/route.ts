import { NextResponse } from 'next/server';
import { query } from '@/db';
import { invoke } from '@/ai/router';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';


export async function POST(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.staff.invoke });
  if (guard.response) return guard.response;

  const { id: idStr } = await params;
  const id = parseInt(idStr, 10);
  if (isNaN(id)) return NextResponse.json({ ok: false, error: 'invalid id' }, { status: 400 });

  const body = await req.json().catch(() => ({}));

  const r = await query(
    `SELECT s.id, s.name, s.system_prompt, s.capabilities,
            p.id as p_id, p.type as p_type, p.base_url as p_base_url, p.api_key_enc as p_api_key_enc,
            m.id as m_id, m.name as m_name
     FROM ai_staff s
     LEFT JOIN ai_providers p ON p.id = s.default_provider_id AND p.enabled = true
     LEFT JOIN ai_models m ON m.id = s.default_model_id AND m.enabled = true
     WHERE s.id = $1 AND s.enabled = true`,
    [id],
  );
  if (r.rows.length === 0) return NextResponse.json({ ok: false, error: 'staff not found' }, { status: 404 });

  const staff = r.rows[0];
  if (!staff.p_id || !staff.m_id) {
    return NextResponse.json({ ok: false, error: 'staff has no default provider/model' }, { status: 400 });
  }

  const sectionKey = body.sectionKey || `staff-test:${staff.name.toLowerCase().replace(/\s+/g, '-')}`;
  await query(
    `INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, staff_id, params_json, priority)
     VALUES ($1,$2,$3,$4,$5,$6,$7)
     ON CONFLICT (section_key, task_type, priority) DO NOTHING`,
    [sectionKey, body.task || 'chat', staff.p_id, staff.m_id, staff.id, JSON.stringify(body.params || {}), 50],
  );

  const result = await invoke(
    sectionKey,
    (body.task || 'chat') as 'embed' | 'chat' | 'vision',
    {
      text: body.text,
      messages: body.messages,
      systemPrompt: staff.system_prompt,
      temperature: body.temperature,
      maxTokens: body.maxTokens,
    },
    { staffId: staff.id },
  );
  return NextResponse.json({ ok: true, result, sectionKey });
}