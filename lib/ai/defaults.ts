export const DEFAULT_CHAT_MODEL = 'MiniMax-M3';

export interface ThinkingParams {
  temperature: number;
  maxTokens: number;
  reasoning_effort: 'low' | 'medium' | 'high';
}

export const THINKING_PRESETS: Record<ThinkingParams['reasoning_effort'], ThinkingParams> = {
  low:    { temperature: 0.7, maxTokens: 2000, reasoning_effort: 'low' },
  medium: { temperature: 0.5, maxTokens: 3000, reasoning_effort: 'medium' },
  high:   { temperature: 0.3, maxTokens: 4000, reasoning_effort: 'high' },
};

export const DEFAULT_THINKING: ThinkingParams = THINKING_PRESETS.high;