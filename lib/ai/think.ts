// Server-side guard for model "thinking" markers. Some providers (Qwen, DeepSeek
// variants, MiniMax in chat mode) emit free-form <think>...</think> reasoning
// tokens in the answer stream. We always strip them before parsing typed blocks
// or feeding the answer to the user.

export const THINK_OPEN = '<think>';
export const THINK_CLOSE = '</think>';

export function stripThinkTags(text: string | null | undefined): string {
  if (!text) return '';
  return text
    .replace(/<think>[\s\S]*?<\/think>/gi, '')
    .replace(/<\/?think>/gi, '')
    .trim();
}