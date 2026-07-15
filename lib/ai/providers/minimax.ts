// MiniMax preset — thin wrapper over OpenAI-compat with sensible defaults
// Users can override baseUrl and model name in the UI; this just pre-fills the form.

import { openaiChat, openaiEmbed, openaiListModels, type OpenAIProviderConfig } from './openai';

export const MINIMAX_PRESET = {
  type: 'minimax' as const,
  baseUrl: 'https://api.minimax.chat/v1',
  suggestedModel: 'MiniMax-M3',
  notes: 'OpenAI-compatible endpoint; enter your MiniMax API key.',
};

export async function minimaxEmbed(cfg: OpenAIProviderConfig, model: string, input: string) {
  return openaiEmbed(cfg, model, input);
}

export async function minimaxChat(
  cfg: OpenAIProviderConfig,
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>,
  params: Record<string, any> = {}
) {
  return openaiChat(cfg, model, messages, params);
}

export async function minimaxListModels(cfg: OpenAIProviderConfig) {
  return openaiListModels(cfg);
}