import { NextResponse } from 'next/server';
import { query } from '@/db';
import { VISION_MODELS_CACHE_TAG } from '@/ai/loadVisionModels';
import { providerApiKey } from '@/ai/providerKey';
import { apiGuard } from '@/server/apiGuard';
import { revalidateTag } from 'next/cache';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface OpenRouterModel {
  id: string;
  name?: string;
  description?: string;
  context_length?: number;
  architecture?: { input_modalities?: string[] };
  pricing?: Record<string, string>;
  supported_parameters?: string[];
  reasoning?: { supported_efforts?: string[] };
}

function capabilities(model: OpenRouterModel): string[] {
  const id = model.id.toLowerCase();
  const input = model.architecture?.input_modalities ?? [];
  if (id === 'openrouter/free') return ['chat', 'vision'];
  const caps = id.includes('embed') ? ['embed'] : ['chat'];
  if (input.includes('image') || input.includes('file')) caps.push('vision');
  return caps;
}

function isFree(model: OpenRouterModel): boolean {
  if (model.id === 'openrouter/free' || model.id.endsWith(':free')) return true;
  const pricing = model.pricing ?? {};
  return pricing.prompt === '0' && pricing.completion === '0';
}

export async function POST(_req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(_req, { perm: 'ai:model:sync::allow' });
  if (guard.response) return guard.response;

  const { id } = await params;
  const provider = await query<{ name: string; type: string; base_url: string; api_key_enc: Buffer | null }>(
    'SELECT name, type, base_url, api_key_enc FROM ai_providers WHERE id = $1',
    [id],
  );
  const row = provider.rows[0];
  if (!row) return NextResponse.json({ error: 'provider not found' }, { status: 404 });
  if (row.type !== 'openai_compat') return NextResponse.json({ error: 'model sync currently supports OpenAI-compatible providers' }, { status: 400 });

  const apiKey = await providerApiKey(row.name, row.api_key_enc);
  const response = await fetch(`${row.base_url.replace(/\/$/, '')}/models`, {
    headers: apiKey ? {
      Authorization: `Bearer ${apiKey}`,
      ...(row.name.toLowerCase() === 'openrouter'
        ? {
            'HTTP-Referer': process.env.OPENROUTER_HTTP_REFERER || 'http://localhost:3004',
            'X-OpenRouter-Title': process.env.OPENROUTER_TITLE || 'Folio',
          }
        : {}),
    } : undefined,
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    const upstream = body?.error ?? body;
    return NextResponse.json({
      error: 'provider_unavailable',
      provider: row.name,
      status: response.status,
      upstreamCode: upstream?.code ?? null,
      upstreamMessage: typeof upstream?.message === 'string' ? upstream.message : `Provider returned HTTP ${response.status}`,
    }, { status: 502 });
  }
  const payload = await response.json() as { data?: OpenRouterModel[] };
  const models = Array.isArray(payload.data) ? payload.data : [];
  if (row.name.toLowerCase() === 'openrouter' && !models.some((model) => model.id === 'openrouter/free')) {
    models.push({
      id: 'openrouter/free',
      name: 'Free Models Router',
      description: 'Routes each request to an available free OpenRouter vision model.',
      context_length: 200000,
      architecture: { input_modalities: ['text', 'image'] },
      pricing: { prompt: '0', completion: '0' },
    });
  }

  let synced = 0;
  for (const model of models) {
    const modelCaps = capabilities(model);
    const reasoningLevels = (model.reasoning?.supported_efforts ?? []).filter((level) => ['low', 'medium', 'high'].includes(level));
    await query(
      `INSERT INTO ai_models
        (provider_id, name, capabilities, context_window, defaults_json, description, enabled, reasoning_levels, is_free, synced_at)
       VALUES ($1,$2,$3,$4,$5,$6,true,$7,$8,now())
       ON CONFLICT (provider_id, name) DO UPDATE SET
         capabilities = EXCLUDED.capabilities,
         context_window = EXCLUDED.context_window,
         defaults_json = EXCLUDED.defaults_json,
         description = EXCLUDED.description,
         reasoning_levels = EXCLUDED.reasoning_levels,
         is_free = EXCLUDED.is_free,
         synced_at = now()`,
      [
        id,
        model.id,
        modelCaps,
        model.context_length ?? null,
        JSON.stringify({ supported_parameters: model.supported_parameters ?? [], pricing: model.pricing ?? {} }),
        model.name ? `${model.name}${model.description ? ` — ${model.description}` : ''}` : model.description ?? null,
        reasoningLevels,
        isFree(model),
      ],
    );
    synced += 1;
  }

  revalidateTag(VISION_MODELS_CACHE_TAG, 'max');
  return NextResponse.json({ ok: true, synced, provider: row.name });
}
