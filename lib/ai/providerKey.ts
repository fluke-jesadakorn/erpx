import 'server-only';

import { decryptKey } from './crypto';

export function providerEnvApiKey(name: string): string | null {
  const provider = name.toLowerCase();
  if (provider === 'openrouter') return process.env.OPENROUTER_API_KEY || process.env.OPENAI_API_KEY || null;
  if (provider === 'minimax') return process.env.MINIMAX_API_KEY || null;
  if (provider === 'openai') return process.env.OPENAI_API_KEY || null;
  return null;
}

export async function providerApiKey(name: string, encrypted: Buffer | null): Promise<string | null> {
  try {
    const stored = await decryptKey(encrypted);
    if (stored) return stored;
  } catch {}
  return providerEnvApiKey(name);
}
