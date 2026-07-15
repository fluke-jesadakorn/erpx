// AI Router — resolves (section_key, task) → provider+model, invokes, logs.
// Single entry point for all AI calls across the system.

import { query } from '../db';
import { decryptKey } from './crypto';
import * as ollama from './providers/ollama';
import * as openai from './providers/openai';
import type { AITask } from './sections';

export interface InvokeInput {
  text?: string;
  messages?: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  images?: string[];                  // base64-encoded image data URIs (vision only)
  modelOverride?: string;             // force this exact model by name; bypasses assignment lookup
  lang?: 'en' | 'th' | 'de';          // secondary locale — for provider-level adaptation
}

export interface InvokeResult {
  ok: boolean;
  text?: string;
  embedding?: number[];
  tokens?: { prompt?: number; response?: number };
  error?: string;
  statusCode?: number;
  upstreamCode?: number;
  upstreamMessage?: string;
  providerId?: number;
  modelId?: number;
  modelName?: string;
  latencyMs?: number;
}

interface ResolvedAssignment {
  provider: {
    id: number;
    name: string;
    type: 'ollama' | 'openai_compat' | 'minimax';
    base_url: string;
    api_key: string | null;
  };
  model: { id: number; name: string; defaults_json: any };
  params: Record<string, any>;
}

const FALLBACK_ENV = {
  ollama: {
    baseUrl: process.env.OLLAMA_URL || 'http://localhost:11434',
    embedModel: process.env.EMBED_MODEL || 'bge-m3:latest',
    chatModel: process.env.OLLAMA_CHAT_MODEL || 'qwen2.5:7b',
  },
  openai_compat: {
    baseUrl: process.env.OPENAI_BASE_URL || 'https://openrouter.ai/api/v1',
    apiKey: process.env.OPENROUTER_API_KEY || process.env.OPENAI_API_KEY || null,
    chatModel: process.env.OPENROUTER_MODEL || 'google/gemini-2.5-flash',
  },
  minimax: {
    baseUrl: process.env.MINIMAX_BASE_URL || 'https://api.minimax.chat/v1',
    apiKey: process.env.MINIMAX_API_KEY || null,
    chatModel: process.env.MINIMAX_MODEL || 'MiniMax-M3',
  },
};

export async function resolve(sectionKey: string, task: AITask, modelOverride?: string): Promise<ResolvedAssignment | null> {
  // 0. Model override (per-call): look up the model by name and use its provider directly.
  if (modelOverride) {
    const r = await query<{
      p_id: number; p_name: string; p_type: 'ollama' | 'openai_compat' | 'minimax';
      p_base_url: string; p_api_key_enc: Buffer | null;
      m_id: number; m_name: string; m_defaults: any;
    }>(
      `SELECT p.id as p_id, p.name as p_name, p.type as p_type, p.base_url as p_base_url, p.api_key_enc as p_api_key_enc,
              m.id as m_id, m.name as m_name, m.defaults_json as m_defaults
         FROM ai_models m
         LEFT JOIN ai_providers p ON p.id = m.provider_id AND p.enabled = true
        WHERE m.name = $1 AND m.enabled = true
        LIMIT 1`,
      [modelOverride]
    );
    if (r.rows.length > 0 && r.rows[0].p_id && r.rows[0].m_id) {
      const row = r.rows[0];
      return {
        provider: {
          id: row.p_id,
          name: row.p_name,
          type: row.p_type,
          base_url: row.p_base_url,
          api_key: await decryptKey(row.p_api_key_enc),
        },
        model: { id: row.m_id, name: row.m_name, defaults_json: row.m_defaults || {} },
        params: {},
      };
    }
    // Model not registered — fall through to assignment lookup so the user gets a
    // meaningful "no AI configured" error rather than a silent null.
  }

  // 1. Look up the highest-priority enabled assignment
  const res = await query(
    `SELECT
       p.id as p_id, p.name as p_name, p.type as p_type, p.base_url as p_base_url, p.api_key_enc as p_api_key_enc,
       m.id as m_id, m.name as m_name, m.defaults_json as m_defaults,
       a.params_json as a_params
     FROM ai_assignments a
     LEFT JOIN ai_providers p ON p.id = a.provider_id AND p.enabled = true
     LEFT JOIN ai_models m ON m.id = a.model_id AND m.enabled = true
     WHERE a.section_key = $1 AND a.task_type = $2 AND a.enabled = true
     ORDER BY a.priority ASC, a.id ASC
     LIMIT 1`,
    [sectionKey, task]
  );

  if (res.rows.length > 0 && res.rows[0].p_id && res.rows[0].m_id) {
    const r = res.rows[0];
    return {
      provider: {
        id: r.p_id,
        name: r.p_name,
        type: r.p_type,
        base_url: r.p_base_url,
        api_key: await decryptKey(r.p_api_key_enc),
      },
      model: { id: r.m_id, name: r.m_name, defaults_json: r.m_defaults || {} },
      params: r.a_params || {},
    };
  }

  // 2. Env fallback
  if (task === 'embed' && FALLBACK_ENV.ollama.baseUrl) {
    return {
      provider: {
        id: 0,
        name: 'env:ollama',
        type: 'ollama',
        base_url: FALLBACK_ENV.ollama.baseUrl,
        api_key: null,
      },
      model: { id: 0, name: FALLBACK_ENV.ollama.embedModel, defaults_json: {} },
      params: {},
    };
  }
  if ((task === 'chat' || task === 'vision') && FALLBACK_ENV.openai_compat.apiKey) {
    return {
      provider: {
        id: 0,
        name: 'env:openai_compat',
        type: 'openai_compat',
        base_url: FALLBACK_ENV.openai_compat.baseUrl,
        api_key: FALLBACK_ENV.openai_compat.apiKey,
      },
      model: { id: 0, name: FALLBACK_ENV.openai_compat.chatModel, defaults_json: {} },
      params: {},
    };
  }
  if ((task === 'chat' || task === 'vision') && FALLBACK_ENV.minimax.apiKey) {
    return {
      provider: {
        id: 0,
        name: 'env:minimax',
        type: 'minimax',
        base_url: FALLBACK_ENV.minimax.baseUrl,
        api_key: FALLBACK_ENV.minimax.apiKey,
      },
      model: { id: 0, name: FALLBACK_ENV.minimax.chatModel, defaults_json: {} },
      params: {},
    };
  }
  if ((task === 'chat' || task === 'vision') && FALLBACK_ENV.ollama.baseUrl) {
    return {
      provider: {
        id: 0,
        name: 'env:ollama',
        type: 'ollama',
        base_url: FALLBACK_ENV.ollama.baseUrl,
        api_key: null,
      },
      model: { id: 0, name: FALLBACK_ENV.ollama.chatModel, defaults_json: {} },
      params: {},
    };
  }

  return null;
}

async function callProvider(
  res: ResolvedAssignment,
  task: AITask,
  input: InvokeInput
): Promise<{ text?: string; embedding?: number[] }> {
  const params = { ...(res.model.defaults_json || {}), ...(res.params || {}), ...stripSystemFromInput(input) };

  if (res.provider.type === 'ollama') {
    if (task === 'embed') {
      if (!input.text) throw new Error('embed task requires input.text');
      const embedding = await ollama.ollamaEmbed({ baseUrl: res.provider.base_url }, res.model.name, input.text);
      return { embedding };
    }
    const messages = buildMessages(input, !!input.images?.length);
    if (input.images?.length && messages[messages.length - 1]) {
      (messages[messages.length - 1] as any).images = input.images.map(stripDataUriPrefix);
    }
    const text = await ollama.ollamaChat({ baseUrl: res.provider.base_url }, res.model.name, messages, {
      ...(params.temperature != null ? { temperature: params.temperature } : {}),
      ...(params.max_tokens != null ? { options: { num_predict: params.max_tokens } } : {}),
    });
    return { text };
  }

  // openai_compat + minimax share the same code path
  const cfg = { baseUrl: res.provider.base_url, apiKey: res.provider.api_key };
  if (task === 'embed') {
    if (!input.text) throw new Error('embed task requires input.text');
    const embedding = await openai.openaiEmbed(cfg, res.model.name, input.text);
    return { embedding };
  }
  const messages = buildMessages(input, !!input.images?.length, input.images);
  const text = await openai.openaiChat(cfg, res.model.name, messages, {
    ...(params.temperature != null ? { temperature: params.temperature } : {}),
    ...(params.maxTokens != null ? { max_tokens: params.maxTokens } : {}),
    ...(params.top_p != null ? { top_p: params.top_p } : {}),
  });
  return { text };
}

function stripDataUriPrefix(s: string): string {
  return s.startsWith('data:') ? s.split(',', 2)[1] || s : s;
}

function stripSystemFromInput(input: InvokeInput) {
  const out: Record<string, any> = {};
  if (input.temperature != null) out.temperature = input.temperature;
  if (input.maxTokens != null) out.maxTokens = input.maxTokens;
  if (input.topP != null) out.topP = input.topP;
  return out;
}

function buildMessages(input: InvokeInput, hasImages = false, imageUrls?: string[]) {
  const msgs: Array<{ role: 'system' | 'user' | 'assistant'; content: any }> = [];
  if (input.systemPrompt) msgs.push({ role: 'system', content: input.systemPrompt });
  if (input.messages) msgs.push(...input.messages.map(m => ({ ...m })));
  else if (input.text) {
    if (hasImages && imageUrls?.length) {
      msgs.push({
        role: 'user',
        content: [
          { type: 'text', text: input.text },
          ...imageUrls.map(url => ({ type: 'image_url', image_url: { url } })),
        ],
      });
    } else {
      msgs.push({ role: 'user', content: input.text });
    }
  }
  if (msgs.length === 0) throw new Error('No input text or messages provided');
  return msgs;
}

function approxTokens(s: string | undefined): number {
  if (!s) return 0;
  return Math.ceil(s.length / 4);
}

// Server-action-friendly alias
export const aiInvoke = invoke;
export async function invoke(
  sectionKey: string,
  task: AITask,
  input: InvokeInput,
  meta: { actorId?: number; staffId?: number } = {}
): Promise<InvokeResult> {
  const t0 = Date.now();
  const res = await resolve(sectionKey, task, input.modelOverride);

  if (!res) {
    await logInvocation({
      sectionKey, task, status: 'error', error: 'No provider/model resolved for section',
      promptExcerpt: input.text || input.messages?.map(m => m.content).join('\n'),
      actorId: meta.actorId, staffId: meta.staffId,
    });
    return { ok: false, error: `No AI provider configured for ${sectionKey} (${task})` };
  }

  try {
    const out = await callProvider(res, task, input);
    const latencyMs = Date.now() - t0;
    const text = out.text;
    const promptExcerpt = (input.text || input.messages?.map(m => m.content).join('\n') || '').slice(0, 500);
    const responseExcerpt = (text || '').slice(0, 500);
    await logInvocation({
      sectionKey, task, status: 'ok',
      providerId: res.provider.id, modelId: res.model.id,
      staffId: meta.staffId, actorId: meta.actorId,
      promptTokens: approxTokens(promptExcerpt),
      responseTokens: approxTokens(text),
      latencyMs,
      promptExcerpt, responseExcerpt,
    });
    return { ok: true, text, embedding: out.embedding, providerId: res.provider.id, modelId: res.model.id, modelName: res.model.name, latencyMs };
  } catch (e: any) {
    const latencyMs = Date.now() - t0;
    const statusCode = e?.response?.status ?? e?.status ?? null;
    const upstreamCode = e?.response?.data?.error?.code ?? e?.response?.data?.code ?? null;
    const upstreamMessage = e?.response?.data?.error?.message ?? e?.response?.data?.message ?? null;
    await logInvocation({
      sectionKey, task, status: 'error',
      providerId: res.provider.id, modelId: res.model.id,
      staffId: meta.staffId, actorId: meta.actorId,
      latencyMs, error: e?.message || String(e),
      promptExcerpt: (input.text || '').slice(0, 500),
    });
    return {
      ok: false,
      error: e?.message || String(e),
      statusCode: statusCode ?? undefined,
      upstreamCode: upstreamCode ?? undefined,
      upstreamMessage: upstreamMessage ?? undefined,
      providerId: res.provider.id, modelId: res.model.id, modelName: res.model.name, latencyMs,
    };
  }
}

async function logInvocation(opts: {
  sectionKey: string;
  task: string;
  status: string;
  providerId?: number;
  modelId?: number;
  staffId?: number;
  actorId?: number;
  promptTokens?: number;
  responseTokens?: number;
  latencyMs?: number;
  error?: string;
  promptExcerpt?: string;
  responseExcerpt?: string;
}) {
  try {
    await query(
      `INSERT INTO ai_invocations
        (staff_id, section_key, task_type, provider_id, model_id, prompt_tokens, response_tokens, latency_ms, status, error, prompt_excerpt, response_excerpt, actor_id)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)`,
      [
        opts.staffId ?? null,
        opts.sectionKey,
        opts.task,
        opts.providerId ?? null,
        opts.modelId ?? null,
        opts.promptTokens ?? null,
        opts.responseTokens ?? null,
        opts.latencyMs ?? null,
        opts.status,
        opts.error ?? null,
        opts.promptExcerpt ?? null,
        opts.responseExcerpt ?? null,
        opts.actorId ?? null,
      ]
    );
  } catch (e) {
    console.error('Failed to log AI invocation:', e);
  }
}