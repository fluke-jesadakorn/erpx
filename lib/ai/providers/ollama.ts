// Ollama provider client
// Compatible with Ollama's native HTTP API (/api/embed, /api/chat, /api/generate)

import axios from 'axios';

export interface OllamaProviderConfig {
  baseUrl: string;
}

export async function ollamaEmbed(cfg: OllamaProviderConfig, model: string, input: string): Promise<number[]> {
  const res = await axios.post(`${cfg.baseUrl}/api/embed`, { model, input }, { timeout: 60_000 });
  const data = res.data;
  if (Array.isArray(data?.embeddings) && data.embeddings[0]) return data.embeddings[0];
  if (Array.isArray(data?.embedding)) return data.embedding;
  throw new Error('Ollama /api/embed returned no embeddings');
}

export async function ollamaChat(
  cfg: OllamaProviderConfig,
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>,
  params: Record<string, any> = {}
): Promise<string> {
  const res = await axios.post(
    `${cfg.baseUrl}/api/chat`,
    { model, messages, stream: false, ...params },
    { timeout: 120_000 }
  );
  const content = res.data?.message?.content;
  if (typeof content !== 'string') throw new Error('Ollama /api/chat returned no message content');
  return content;
}

export async function ollamaListModels(cfg: OllamaProviderConfig): Promise<string[]> {
  const res = await axios.get(`${cfg.baseUrl}/api/tags`, { timeout: 10_000 });
  return Array.isArray(res.data?.models) ? res.data.models.map((m: any) => m.name) : [];
}