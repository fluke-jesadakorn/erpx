// OpenAI-compatible provider client
// Works for OpenAI, OpenRouter, and MiniMax (which exposes /v1/* OpenAI-shape endpoints)

import axios from 'axios';

export interface OpenAIProviderConfig {
  baseUrl: string;
  apiKey: string | null;
  providerName?: string;
}

function headers(cfg: OpenAIProviderConfig): Record<string, string> {
  const h: Record<string, string> = { 'Content-Type': 'application/json' };
  if (cfg.apiKey) h['Authorization'] = `Bearer ${cfg.apiKey}`;
  if (cfg.providerName?.toLowerCase() === 'openrouter' || cfg.baseUrl.includes('openrouter.ai')) {
    h['HTTP-Referer'] = process.env.OPENROUTER_HTTP_REFERER || 'http://localhost:3004';
    h['X-OpenRouter-Title'] = process.env.OPENROUTER_TITLE || 'Folio';
  }
  return h;
}

export class OpenAIProviderError extends Error {
  statusCode?: number;
  upstreamCode?: number | string;
  upstreamMessage?: string;
  metadata?: Record<string, unknown>;

  constructor(message: string, fields: Pick<OpenAIProviderError, 'statusCode' | 'upstreamCode' | 'upstreamMessage' | 'metadata'> = {}) {
    super(message);
    this.name = 'OpenAIProviderError';
    Object.assign(this, fields);
  }
}

function errorFields(error: any): Pick<OpenAIProviderError, 'statusCode' | 'upstreamCode' | 'upstreamMessage' | 'metadata'> {
  const body = error?.response?.data ?? error?.body ?? error?.data ?? {};
  const nested = body?.error ?? body;
  const statusCode = error?.response?.status ?? error?.status ?? undefined;
  const upstreamCode = nested?.code ?? body?.code ?? undefined;
  const upstreamMessage = nested?.message ?? body?.message ?? error?.message ?? 'OpenAI-compatible provider request failed';
  const metadata = nested?.metadata && typeof nested.metadata === 'object' ? nested.metadata : undefined;
  return { statusCode, upstreamCode, upstreamMessage, metadata };
}

function normalizeError(error: unknown): OpenAIProviderError {
  if (error instanceof OpenAIProviderError) return error;
  const fields = errorFields(error);
  return new OpenAIProviderError(fields.upstreamMessage || 'OpenAI-compatible provider request failed', fields);
}

function contentText(content: unknown): string | null {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return null;
  const parts = content.map((part) => {
    if (typeof part === 'string') return part;
    if (part && typeof part === 'object' && typeof (part as { text?: unknown }).text === 'string') {
      return (part as { text: string }).text;
    }
    return '';
  });
  const text = parts.join('');
  return text || null;
}

export async function openaiEmbed(cfg: OpenAIProviderConfig, model: string, input: string): Promise<number[]> {
  try {
    const res = await axios.post(
      `${cfg.baseUrl}/embeddings`,
      { model, input },
      { headers: headers(cfg), timeout: 60_000 }
    );
    const emb = res.data?.data?.[0]?.embedding;
    if (!Array.isArray(emb)) throw new OpenAIProviderError('OpenAI-compat /embeddings returned no embedding');
    return emb;
  } catch (error) {
    throw normalizeError(error);
  }
}

export async function openaiChat(
  cfg: OpenAIProviderConfig,
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: unknown }>,
  params: Record<string, any> = {}
): Promise<string> {
  try {
    const body = { model, messages, ...params };
    const res = await axios.post(`${cfg.baseUrl}/chat/completions`, body, {
      headers: headers(cfg),
      timeout: 120_000,
    });
    const content = contentText(res.data?.choices?.[0]?.message?.content);
    if (content == null) throw new OpenAIProviderError('OpenAI-compat /chat/completions returned no content');
    return content;
  } catch (error) {
    throw normalizeError(error);
  }
}

export async function openaiListModels(cfg: OpenAIProviderConfig): Promise<string[]> {
  try {
    const res = await axios.get(`${cfg.baseUrl}/models`, {
      headers: headers(cfg),
      timeout: 10_000,
    });
    return Array.isArray(res.data?.data) ? res.data.data.map((m: any) => m.id) : [];
  } catch (error) {
    throw normalizeError(error);
  }
}

export async function* openaiChatStream(
  cfg: OpenAIProviderConfig,
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: unknown }>,
  params: Record<string, any> = {}
): AsyncGenerator<string, void, void> {
  try {
    const res = await fetch(`${cfg.baseUrl}/chat/completions`, {
      method: 'POST',
      headers: headers(cfg),
      body: JSON.stringify({ model, messages, stream: true, ...params }),
    });
    if (!res.ok || !res.body) {
      const body = await res.json().catch(() => ({}));
      throw new OpenAIProviderError(errorFields({ status: res.status, body }).upstreamMessage || `openai-compat stream HTTP ${res.status}`, errorFields({ status: res.status, body }));
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let idx;
      while ((idx = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, idx).trim();
        buf = buf.slice(idx + 1);
        if (!line || !line.startsWith('data:')) continue;
        const payload = line.slice(5).trim();
        if (payload === '[DONE]') return;
        let obj: any;
        try {
          obj = JSON.parse(payload);
        } catch {
          throw new OpenAIProviderError('OpenAI-compatible stream returned malformed JSON', {
            upstreamMessage: 'Malformed streaming response from provider',
          });
        }
        if (obj?.error) {
          throw new OpenAIProviderError(obj.error.message || 'OpenAI-compatible stream failed', {
            upstreamCode: obj.error.code,
            upstreamMessage: obj.error.message,
            metadata: obj.error.metadata,
          });
        }
        const chunk = contentText(obj?.choices?.[0]?.delta?.content);
        if (chunk) yield chunk;
      }
    }
  } catch (error) {
    throw normalizeError(error);
  }
}
