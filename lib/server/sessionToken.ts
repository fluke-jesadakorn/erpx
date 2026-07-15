// HMAC-signed session token for the simulated-actor flow.
// Format: base64url(payloadJson) + "." + base64url(hmacSha256(payload, SESSION_SECRET))
//
// Edge-runtime safe (uses Web Crypto API). Works in middleware and Node server actions alike.

export interface SessionPayload {
  id: string;
  sub: number;
  role: string;
  impersonatorUserId: number | null;
  iat: number;
  exp: number;
}

export const SESSION_COOKIE = 'folio_session';
export const SESSION_TTL_SECONDS = 60 * 60 * 24 * 30;

export function mintSessionId(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let bin = '';
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export const ENC = new TextEncoder();
export const DEC = new TextDecoder();

const B64U = (buf: ArrayBuffer | Uint8Array): string => {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  let bin = '';
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

const B64U_DEC = (s: string): Uint8Array => {
  const padded = s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4);
  const bin = atob(padded);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
};

function secret(): string {
  const s = process.env.SESSION_SECRET;
  if (!s || s.length < 16) {
    throw new Error('SESSION_SECRET is required (min 16 chars). Generate with `openssl rand -hex 32`.');
  }
  return s;
}

let cachedKey: CryptoKey | null = null;
async function key(): Promise<CryptoKey> {
  if (cachedKey) return cachedKey;
  cachedKey = await crypto.subtle.importKey(
    'raw',
    ENC.encode(secret()),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign', 'verify'],
  );
  return cachedKey;
}

async function sign(data: Uint8Array): Promise<Uint8Array> {
  const k = await key();
  const sig = await crypto.subtle.sign('HMAC', k, data as BufferSource);
  return new Uint8Array(sig);
}

function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

export async function signSession(payload: Omit<SessionPayload, 'iat' | 'exp'>): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  const full: SessionPayload = { ...payload, iat: now, exp: now + SESSION_TTL_SECONDS };
  const json = ENC.encode(JSON.stringify(full));
  const head = B64U(json);
  const sig = B64U(await sign(ENC.encode(head)));
  return `${head}.${sig}`;
}

export async function verifySession(token: string | null | undefined): Promise<SessionPayload | null> {
  if (!token) return null;
  const parts = token.split('.');
  if (parts.length !== 2) return null;
  const [head, sig] = parts;
  let expected: Uint8Array;
  let provided: Uint8Array;
  try {
    expected = await sign(ENC.encode(head));
    provided = B64U_DEC(sig);
  } catch {
    return null;
  }
  if (!timingSafeEqual(expected, provided)) return null;
  let payload: SessionPayload;
  try {
    payload = JSON.parse(DEC.decode(B64U_DEC(head))) as SessionPayload;
  } catch {
    return null;
  }
  if (typeof payload.sub !== 'number' || typeof payload.exp !== 'number') return null;
  if (payload.exp < Math.floor(Date.now() / 1000)) return null;
  return payload;
}

export function safeEqual(a: string, b: string): boolean {
  if (typeof a !== 'string' || typeof b !== 'string') return false;
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

export function sessionFromHeaders(
  headers: Record<string, string | string[] | undefined> | Headers,
): string | null {
  const get = (k: string): string | undefined => {
    if (headers instanceof Headers) return headers.get(k) ?? undefined;
    const v = headers[k];
    return Array.isArray(v) ? v[0] : v;
  };
  const raw = get('x-folio-session');
  if (raw) return raw;
  const cookie = get('cookie');
  if (typeof cookie !== 'string') return null;
  const match = cookie.match(/(?:^|;\s*)folio_session=([^;]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}