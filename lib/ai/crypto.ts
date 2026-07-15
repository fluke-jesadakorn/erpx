// Encryption helpers for AI provider API keys
// Uses pgcrypto via DB SQL functions; key never leaves the process unencrypted at rest.

import { query } from '../db';

export function getEncryptionKey(): string {
  const k = process.env.ENCRYPTION_KEY;
  if (!k) {
    throw new Error('ENCRYPTION_KEY env var is required for AI provider API key encryption.');
  }
  return k;
}

export async function encryptKey(plain: string): Promise<Buffer | null> {
  if (!plain) return null;
  const key = getEncryptionKey();
  const res = await query(
    "SELECT ai_encrypt($1, $2) AS ai_encrypt",
    [plain, key]
  );
  return res.rows[0].ai_encrypt;
}

export async function decryptKey(cipher: Buffer | null | undefined): Promise<string | null> {
  if (!cipher) return null;
  const key = getEncryptionKey();
  const res = await query(
    "SELECT ai_decrypt($1, $2) AS ai_decrypt",
    [cipher, key]
  );
  return res.rows[0].ai_decrypt;
}

// Decrypt providers in bulk — used by the router to pick a model at runtime
export async function decryptProviders(rows: any[]): Promise<any[]> {
  return Promise.all(rows.map(async (r) => ({
    ...r,
    api_key: await decryptKey(r.api_key_enc),
  })));
}