// AI Router — resolves (section_key, task) → provider+model, invokes, logs.
// Single entry point for all AI calls across the system.

import { query } from '../db';
import * as ollama from './providers/ollama';
import * as openai from './providers/openai';
import { loadUserAiPreference, type ThinkingLevel } from './preferences';
import { providerApiKey } from './providerKey';
import { DEFAULT_OPENROUTER_MODEL } from './defaults';
import type { AITask, SectionKey } from './sections';
import { assertSection } from './sections';

export type { SectionKey } from './sections';

export interface InvokeInput {
  text?: string;
  messages?: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  images?: string[];                  // base64-encoded image data URIs (vision only)
  modelOverride?: string;             // legacy model name override; validated against the section whitelist for users
  modelId?: number;                   // preferred model id; validated against the section whitelist for users
  thinking?: Exclude<ThinkingLevel, 'auto'>;
  lang?: 'en' | 'th' | 'de';          // secondary locale — for provider-level adaptation
  actorId?: number;                   // caller id — accepted for log correlation, not used by router
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
  providerName?: string;
  providerUnavailable?: boolean;
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
  model: { id: number; name: string; defaults_json: any; reasoning_levels: string[] };
  params: Record<string, any>;
  thinkingLevel: ThinkingLevel;
}

const FALLBACK_ENV = {
  ollama: {
    baseUrl: process.env.OLLAMA_URL || 'http://localhost:11434',
    embedModel: process.env.EMBED_MODEL || 'bge-m3:latest',
    chatModel: process.env.OLLAMA_CHAT_MODEL || 'qwen2.5:7b',
  },
  openai_compat: {
    baseUrl: process.env.OPENROUTER_BASE_URL || process.env.OPENAI_BASE_URL || 'https://openrouter.ai/api/v1',
    apiKey: process.env.OPENROUTER_API_KEY || process.env.OPENAI_API_KEY || null,
    chatModel: process.env.OPENROUTER_MODEL || DEFAULT_OPENROUTER_MODEL,
  },
  minimax: {
    baseUrl: process.env.MINIMAX_BASE_URL || 'https://api.minimax.chat/v1',
    apiKey: process.env.MINIMAX_API_KEY || null,
    chatModel: process.env.MINIMAX_MODEL || 'MiniMax-M3',
  },
};

export interface ResolveOptions {
  modelOverride?: string;
  modelId?: number;
  actorId?: number;
  providerType?: 'ollama' | 'openai_compat' | 'minimax';
}

export async function resolve(
  sectionKey: string | SectionKey,
  task: AITask,
  options: ResolveOptions | string = {},
): Promise<ResolvedAssignment | null> {
  const opts: ResolveOptions = typeof options === 'string' ? { modelOverride: options } : options;
  const userPreference = opts.actorId ? await loadUserAiPreference(opts.actorId, sectionKey) : null;

  // 0. User preference, but only when the preference still points at an enabled
  // IT-whitelisted assignment for this section.
  if (opts.actorId && !opts.providerType && !opts.modelId && !opts.modelOverride) {
    if (userPreference) {
      opts.modelId = userPreference.modelId;
    }
  }

  // 1. Explicit model selection. For authenticated users this query is also
  // the whitelist check; a model name/id cannot bypass IT policy.
  if (opts.modelId || opts.modelOverride) {
    const r = await query<{
      p_id: number; p_name: string; p_type: 'ollama' | 'openai_compat' | 'minimax';
      p_base_url: string; p_api_key_enc: Buffer | null;
      m_id: number; m_name: string; m_defaults: any; m_reasoning_levels: string[];
    }>(
      `SELECT p.id as p_id, p.name as p_name, p.type as p_type, p.base_url as p_base_url, p.api_key_enc as p_api_key_enc,
              m.id as m_id, m.name as m_name, m.defaults_json as m_defaults,
              m.reasoning_levels as m_reasoning_levels
         FROM ai_models m
         JOIN ai_providers p ON p.id = m.provider_id AND p.enabled = true
         ${opts.actorId ? `JOIN ai_assignments allowed ON allowed.model_id = m.id
                              AND (allowed.section_key = $2 OR ($2 LIKE 'chat:%' AND allowed.section_key = 'chat:global'))
                              AND allowed.task_type = $3
                              AND allowed.enabled = true
                              AND allowed.user_selectable = true` : ''}
        WHERE m.enabled = true
          AND ${opts.modelId ? 'm.id = $1' : 'm.name = $1'}
        LIMIT 1`,
      opts.actorId
        ? [opts.modelId ?? opts.modelOverride, sectionKey, task]
        : [opts.modelId ?? opts.modelOverride],
    );
    if (r.rows.length > 0 && r.rows[0].p_id && r.rows[0].m_id) {
      const row = r.rows[0];
      return {
        provider: {
          id: row.p_id,
          name: row.p_name,
          type: row.p_type,
          base_url: row.p_base_url,
          api_key: await providerApiKey(row.p_name, row.p_api_key_enc),
        },
        model: {
          id: row.m_id,
          name: row.m_name,
          defaults_json: row.m_defaults || {},
          reasoning_levels: row.m_reasoning_levels || [],
        },
        params: {},
        thinkingLevel: userPreference?.thinkLevel ?? 'auto',
      };
    }
    // Model not registered or not whitelisted — fall through to the section default.
  }

  // 2. Look up the highest-priority enabled assignment
  const res = await query(
    `SELECT
       p.id as p_id, p.name as p_name, p.type as p_type, p.base_url as p_base_url, p.api_key_enc as p_api_key_enc,
       m.id as m_id, m.name as m_name, m.defaults_json as m_defaults, m.reasoning_levels as m_reasoning_levels,
       a.params_json as a_params
     FROM ai_assignments a
     LEFT JOIN ai_providers p ON p.id = a.provider_id AND p.enabled = true
     LEFT JOIN ai_models m ON m.id = a.model_id AND m.enabled = true
      WHERE (a.section_key = $1 OR ($1 LIKE 'chat:%' AND a.section_key = 'chat:global'))
        AND a.task_type = $2 AND a.enabled = true
        ${opts.providerType ? 'AND p.type = $3' : ''}
     ORDER BY (a.section_key = $1) DESC, a.priority ASC, a.id ASC
     LIMIT 1`,
    opts.providerType ? [sectionKey, task, opts.providerType] : [sectionKey, task],
  );

  if (res.rows.length > 0 && res.rows[0].p_id && res.rows[0].m_id) {
    const r = res.rows[0];
    return {
      provider: {
        id: r.p_id,
        name: r.p_name,
        type: r.p_type,
        base_url: r.p_base_url,
        api_key: await providerApiKey(r.p_name, r.p_api_key_enc),
      },
      model: {
        id: r.m_id,
        name: r.m_name,
        defaults_json: r.m_defaults || {},
        reasoning_levels: r.m_reasoning_levels || [],
      },
      params: r.a_params || {},
      thinkingLevel: userPreference?.thinkLevel ?? 'auto',
    };
  }

  // 3. Env fallback
  if (opts.providerType === 'ollama' && FALLBACK_ENV.ollama.baseUrl && (task === 'chat' || task === 'vision')) {
    return {
      provider: {
        id: 0,
        name: 'env:ollama',
        type: 'ollama',
        base_url: FALLBACK_ENV.ollama.baseUrl,
        api_key: null,
      },
      model: { id: 0, name: FALLBACK_ENV.ollama.chatModel, defaults_json: {}, reasoning_levels: [] },
      params: {},
      thinkingLevel: 'auto',
    };
  }
  if (task === 'embed' && FALLBACK_ENV.ollama.baseUrl) {
    return {
      provider: {
        id: 0,
        name: 'env:ollama',
        type: 'ollama',
        base_url: FALLBACK_ENV.ollama.baseUrl,
        api_key: null,
      },
      model: { id: 0, name: FALLBACK_ENV.ollama.embedModel, defaults_json: {}, reasoning_levels: [] },
      params: {},
      thinkingLevel: 'auto',
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
      model: { id: 0, name: FALLBACK_ENV.openai_compat.chatModel, defaults_json: {}, reasoning_levels: [] },
      params: {},
      thinkingLevel: 'auto',
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
      model: { id: 0, name: FALLBACK_ENV.minimax.chatModel, defaults_json: {}, reasoning_levels: [] },
      params: {},
      thinkingLevel: 'auto',
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
      model: { id: 0, name: FALLBACK_ENV.ollama.chatModel, defaults_json: {}, reasoning_levels: [] },
      params: {},
      thinkingLevel: 'auto',
    };
  }

  return null;
}

async function callProvider(
  res: ResolvedAssignment,
  task: AITask,
  input: InvokeInput
): Promise<{ text?: string; embedding?: number[] }> {
  const params = { ...requestDefaults(res.model.defaults_json), ...(res.params || {}), ...stripSystemFromInput(input) };
  const thinking = input.thinking ?? (res.thinkingLevel === 'auto' ? undefined : res.thinkingLevel);

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
      ...(thinking ? { think: thinking } : {}),
    });
    return { text };
  }

  // openai_compat + minimax share the same code path
  const cfg = { baseUrl: res.provider.base_url, apiKey: res.provider.api_key, providerName: res.provider.name };
  if (task === 'embed') {
    if (!input.text) throw new Error('embed task requires input.text');
    const embedding = await openai.openaiEmbed(cfg, res.model.name, input.text);
    return { embedding };
  }
  const messages = buildMessages(input, !!input.images?.length, input.images);
  const isOpenRouter = isOpenRouterProvider(res);
  const text = await openai.openaiChat(cfg, res.model.name, messages, {
    ...(params.temperature != null ? { temperature: params.temperature } : {}),
    ...(params.maxTokens != null ? { max_tokens: params.maxTokens } : {}),
    ...(params.top_p != null ? { top_p: params.top_p } : {}),
    ...(thinking && isOpenRouter && supportsModelParameter(res, 'reasoning')
      ? { reasoning: { effort: thinking, exclude: true } }
      : {}),
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

function supportsModelParameter(res: ResolvedAssignment, parameter: string): boolean {
  const supported = res.model.defaults_json?.supported_parameters;
  return !Array.isArray(supported) || supported.includes(parameter);
}

function requestDefaults(defaults: Record<string, unknown> | null | undefined): Record<string, any> {
  if (!defaults || typeof defaults !== 'object') return {};
  const { supported_parameters: _supported, pricing: _pricing, ...params } = defaults;
  return params as Record<string, any>;
}

function isOpenRouterProvider(res: ResolvedAssignment): boolean {
  return res.provider.name.toLowerCase() === 'openrouter' || res.provider.base_url.toLowerCase().includes('openrouter.ai');
}

function isProviderAvailabilityFailure(statusCode: number | null): boolean {
  return statusCode == null || statusCode === 401 || statusCode === 403 || statusCode === 408 || statusCode === 429 || statusCode >= 500;
}

function approxTokens(s: string | undefined): number {
  if (!s) return 0;
  return Math.ceil(s.length / 4);
}

async function invokeFallback(
  sectionKey: string | SectionKey,
  task: AITask,
  input: InvokeInput,
  fallback: ResolvedAssignment,
  meta: { actorId?: number; staffId?: number },
  startedAt: number,
): Promise<InvokeResult | null> {
  try {
    const out = await callProvider(fallback, task, input);
    const text = out.text;
    await logInvocation({
      sectionKey, task, status: 'ok',
      providerId: fallback.provider.id, modelId: fallback.model.id,
      staffId: meta.staffId, actorId: meta.actorId,
      promptTokens: approxTokens((input.text || input.messages?.map(m => m.content).join('\n') || '').slice(0, 500)),
      responseTokens: approxTokens(text),
      latencyMs: Date.now() - startedAt,
      promptExcerpt: (input.text || input.messages?.map(m => m.content).join('\n') || '').slice(0, 500),
      responseExcerpt: (text || '').slice(0, 500),
    });
    return {
      ok: true,
      text,
      embedding: out.embedding,
      providerName: fallback.provider.name,
      providerId: fallback.provider.id,
      modelId: fallback.model.id,
      modelName: fallback.model.name,
      latencyMs: Date.now() - startedAt,
    };
  } catch {
    return null;
  }
}

// Server-action-friendly alias
export const aiInvoke = invoke;
export async function invoke(
  sectionKey: string | SectionKey,
  task: AITask,
  input: InvokeInput,
  meta: { actorId?: number; staffId?: number } = {}
): Promise<InvokeResult> {
  assertSection(sectionKey);
  const t0 = Date.now();
  const res = await resolve(sectionKey, task, {
    modelOverride: input.modelOverride,
    modelId: input.modelId,
    actorId: meta.actorId,
  });

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
    const statusCode = e?.statusCode ?? e?.response?.status ?? e?.status ?? null;
    const upstreamCode = e?.upstreamCode ?? e?.response?.data?.error?.code ?? e?.response?.data?.code ?? null;
    const upstreamMessage = e?.upstreamMessage ?? e?.response?.data?.error?.message ?? e?.response?.data?.message ?? null;

    if (isOpenRouterProvider(res) && (task === 'chat' || task === 'vision') && isProviderAvailabilityFailure(statusCode)) {
      const fallbackNames = res.model.name === 'openrouter/free' ? [] : ['openrouter/free'];
      const fallbacks = await Promise.all([
        ...fallbackNames.map((modelOverride) => resolve(sectionKey, task, { modelOverride, actorId: meta.actorId })),
        resolve(sectionKey, task, { actorId: meta.actorId, providerType: 'ollama' }),
      ]);
      for (const fallback of fallbacks) {
        if (!fallback || fallback.model.name === res.model.name) continue;
        const recovered = await invokeFallback(sectionKey, task, input, fallback, meta, t0);
        if (recovered) return recovered;
      }
    }

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
      providerName: res.provider.name,
      providerUnavailable: isOpenRouterProvider(res) && isProviderAvailabilityFailure(statusCode),
      providerId: res.provider.id, modelId: res.model.id, modelName: res.model.name, latencyMs,
    };
  }
}

export interface InvokeStreamInput extends Omit<InvokeInput, 'maxTokens' | 'topP'> {
  maxTokens?: number;
  topP?: number;
}

async function* streamAssignment(
  res: ResolvedAssignment,
  task: AITask,
  input: InvokeStreamInput,
): AsyncGenerator<string, void, void> {
  const params = { ...requestDefaults(res.model.defaults_json), ...(res.params || {}) };
  const temperature = input.temperature ?? (params as any).temperature;
  const maxTokens = input.maxTokens ?? (params as any).maxTokens;
  const thinking = input.thinking ?? (res.thinkingLevel === 'auto' ? undefined : res.thinkingLevel);
  const messages = buildMessages(input, !!input.images?.length, input.images);

  if (res.provider.type === 'ollama') {
    const stream = ollama.ollamaChatStream(
      { baseUrl: res.provider.base_url },
      res.model.name,
      messages,
      {
        ...(temperature != null ? { temperature } : {}),
        ...(maxTokens != null ? { options: { num_predict: maxTokens } } : {}),
        ...(thinking ? { think: thinking } : {}),
      },
    );
    for await (const chunk of stream) yield chunk;
    return;
  }

  const cfg = { baseUrl: res.provider.base_url, apiKey: res.provider.api_key, providerName: res.provider.name };
  const stream = openai.openaiChatStream(cfg, res.model.name, messages, {
    ...(temperature != null ? { temperature } : {}),
    ...(maxTokens != null ? { max_tokens: maxTokens } : {}),
    ...((params as any).top_p != null ? { top_p: (params as any).top_p } : {}),
    ...(thinking && isOpenRouterProvider(res) && supportsModelParameter(res, 'reasoning')
      ? { reasoning: { effort: thinking, exclude: true } }
      : {}),
  });
  for await (const chunk of stream) yield chunk;
}

export async function* invokeStream(
  sectionKey: SectionKey | string,
  task: AITask,
  input: InvokeStreamInput,
  meta: { actorId?: number; staffId?: number } = {}
): AsyncGenerator<string, void, void> {
  const res = await resolve(sectionKey, task, {
    modelOverride: input.modelOverride,
    modelId: input.modelId,
    actorId: meta.actorId,
  });
  if (!res) {
    await logInvocation({
      sectionKey, task, status: 'error', error: 'No provider/model resolved for section',
      promptExcerpt: input.text || input.messages?.map(m => m.content).join('\n'),
      actorId: meta.actorId, staffId: meta.staffId,
    });
    throw new Error(`No AI provider configured for ${sectionKey} (${task})`);
  }

  if (res.provider.type !== 'ollama' && res.provider.type !== 'openai_compat' && res.provider.type !== 'minimax') {
    throw new Error(`invokeStream: unsupported provider type "${res.provider.type}"`);
  }

  const t0 = Date.now();
  let fullText = '';

  try {
    for await (const chunk of streamAssignment(res, task, input)) {
      fullText += chunk;
      yield chunk;
    }

    const latencyMs = Date.now() - t0;
    await logInvocation({
      sectionKey, task, status: 'ok',
      providerId: res.provider.id, modelId: res.model.id,
      staffId: meta.staffId, actorId: meta.actorId,
      promptTokens: approxTokens(input.text || input.messages?.map(m => m.content).join('\n') || ''),
      responseTokens: approxTokens(fullText),
      latencyMs,
      promptExcerpt: (input.text || '').slice(0, 500),
      responseExcerpt: fullText.slice(0, 500),
    });
  } catch (e: any) {
    const statusCode = e?.statusCode ?? e?.response?.status ?? e?.status ?? null;
    const upstreamCode = e?.upstreamCode ?? e?.response?.data?.error?.code ?? e?.response?.data?.code ?? null;
    const upstreamMessage = e?.upstreamMessage ?? e?.response?.data?.error?.message ?? e?.response?.data?.message ?? null;

    if (!fullText && isOpenRouterProvider(res) && isProviderAvailabilityFailure(statusCode)) {
      const fallbackNames = res.model.name === 'openrouter/free' ? [] : ['openrouter/free'];
      const fallbacks = await Promise.all([
        ...fallbackNames.map((modelOverride) => resolve(sectionKey, task, { modelOverride, actorId: meta.actorId })),
        resolve(sectionKey, task, { actorId: meta.actorId, providerType: 'ollama' }),
      ]);
      for (const fallback of fallbacks) {
        if (!fallback || fallback.model.name === res.model.name) continue;
        let fallbackText = '';
        try {
          for await (const chunk of streamAssignment(fallback, task, input)) {
            fallbackText += chunk;
            yield chunk;
          }
          await logInvocation({
            sectionKey, task, status: 'ok',
            providerId: fallback.provider.id, modelId: fallback.model.id,
            staffId: meta.staffId, actorId: meta.actorId,
            promptTokens: approxTokens(input.text || input.messages?.map(m => m.content).join('\n') || ''),
            responseTokens: approxTokens(fallbackText),
            latencyMs: Date.now() - t0,
            promptExcerpt: (input.text || '').slice(0, 500),
            responseExcerpt: fallbackText.slice(0, 500),
          });
          return;
        } catch {
          if (fallbackText) break;
        }
      }
    }

    await logInvocation({
      sectionKey, task, status: 'error',
      providerId: res.provider.id, modelId: res.model.id,
      staffId: meta.staffId, actorId: meta.actorId,
      latencyMs: Date.now() - t0,
      error: e?.message || String(e),
      promptExcerpt: (input.text || '').slice(0, 500),
    });
    if (e && typeof e === 'object') {
      e.statusCode = statusCode ?? undefined;
      e.upstreamCode = upstreamCode ?? undefined;
      e.upstreamMessage = upstreamMessage ?? undefined;
      e.providerName = res.provider.name;
      e.modelName = res.model.name;
    }
    throw e;
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
    const id = (value?: number) => value && value > 0 ? value : null;
    await query(
      `INSERT INTO ai_invocations
        (staff_id, section_key, task_type, provider_id, model_id, prompt_tokens, response_tokens, latency_ms, status, error, prompt_excerpt, response_excerpt, actor_id)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)`,
      [
        id(opts.staffId),
        opts.sectionKey,
        opts.task,
        id(opts.providerId),
        id(opts.modelId),
        opts.promptTokens ?? null,
        opts.responseTokens ?? null,
        opts.latencyMs ?? null,
        opts.status,
        opts.error ?? null,
        opts.promptExcerpt ?? null,
        opts.responseExcerpt ?? null,
        id(opts.actorId),
      ]
    );
  } catch (e) {
    console.error('Failed to log AI invocation:', e);
  }
}
