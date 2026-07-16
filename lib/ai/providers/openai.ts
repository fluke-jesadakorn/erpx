// OpenAI-compatible provider client
// Works for OpenAI, OpenRouter, and MiniMax (which exposes /v1/* OpenAI-shape endpoints)

import axios from 'axios';

export interface OpenAIProviderConfig {
  baseUrl: string;
  apiKey: string | null;
}

function headers(cfg: OpenAIProviderConfig): Record<string, string> {
  const h: Record<string, string> = { 'Content-Type': 'application/json' };
  if (cfg.apiKey) h['Authorization'] = `Bearer ${cfg.apiKey}`;
  return h;
}

export async function openaiEmbed(cfg: OpenAIProviderConfig, model: string, input: string): Promise<number[]> {
  const res = await axios.post(
    `${cfg.baseUrl}/embeddings`,
    { model, input },
    { headers: headers(cfg), timeout: 60_000 }
  );
  const emb = res.data?.data?.[0]?.embedding;
  if (!Array.isArray(emb)) throw new Error('OpenAI-compat /embeddings returned no embedding');
  return emb;
}

export async function openaiChat(
  cfg: OpenAIProviderConfig,
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>,
  params: Record<string, any> = {}
): Promise<string> {
  const body = { model, messages, ...params };
  const res = await axios.post(`${cfg.baseUrl}/chat/completions`, body, {
    headers: headers(cfg),
    timeout: 120_000,
  });
  const content = res.data?.choices?.[0]?.message?.content;
  if (typeof content !== 'string') throw new Error('OpenAI-compat /chat/completions returned no content');
  return content;
}

export async function openaiListModels(cfg: OpenAIProviderConfig): Promise<string[]> {
  const res = await axios.get(`${cfg.baseUrl}/models`, {
    headers: headers(cfg),
    timeout: 10_000,
  });
  return Array.isArray(res.data?.data) ? res.data.data.map((m: any) => m.id) : [];
}

export async function* openaiChatStream(
  cfg: OpenAIProviderConfig,
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>,
  params: Record<string, any> = {}
): AsyncGenerator<string, void, void> {
  const res = await fetch(`${cfg.baseUrl}/chat/completions`, {
    method: 'POST',
    headers: headers(cfg),
    body: JSON.stringify({ model, messages, stream: true, ...params }),
  });
  if (!res.ok || !res.body) throw new Error(`openai-compat stream HTTP ${res.status}`);
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
      try {
        const obj = JSON.parse(payload);
        const chunk = obj?.choices?.[0]?.delta?.content;
        if (typeof chunk === 'string' && chunk.length > 0) yield chunk;
      } catch { /* skip */ }
    }
  }
}