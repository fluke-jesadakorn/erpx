import { verifyHmacSha256 } from './verify';

export const LINE_SIGNATURE_HEADER = 'x-line-signature';

export function verifyLineSignature(
  rawBody: string,
  signatureHeader: string | null,
  secret: string,
): boolean {
  if (!signatureHeader) return false;
  return verifyHmacSha256(rawBody, signatureHeader, secret);
}

export interface LineEvent {
  type?: string;
  webhookEventId?: string;
  message?: { id?: string; type?: string };
  source?: { userId?: string; type?: string };
  timestamp?: number;
}

export interface LineBody {
  destination?: string;
  events?: LineEvent[];
}

export function lineEventType(body: LineBody): string {
  const e = body.events?.[0];
  if (!e) return 'unknown';
  if (e.type === 'follow') return 'follow';
  if (e.type === 'unfollow') return 'unfollow';
  if (e.type === 'join') return 'join';
  if (e.type === 'leave') return 'leave';
  if (e.type === 'message') return `message:${e.message?.type ?? 'unknown'}`;
  return e.type ?? 'unknown';
}

export function lineExternalId(body: LineBody): string | null {
  const e = body.events?.[0];
  if (!e) return null;
  return e.message?.id ?? e.webhookEventId ?? null;
}