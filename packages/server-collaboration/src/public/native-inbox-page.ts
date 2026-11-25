import 'server-only';
import { cookies, headers } from "next/headers";
import { ApiError } from '@erpx/server-runtime/public/http';
import type { InboxItem } from "@erpx/contracts/inbox";

/** Strict owner transport for the native Collaboration inbox reads.
 * The workplace SSR inbox screen is native-only through these SSR variants;
 * configuration or transport failures fail closed (503) here. No TS read
 * fallback exists behind them. */
export type NativeInboxConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeInboxConfiguration {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  let value: NativeInboxConfiguration;
  try {
    value = JSON.parse(raw) as NativeInboxConfiguration;
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin ||
    !Number.isInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000
  )
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  return value;
}

export function nativeInboxAvailable(): boolean {
  return process.env.ERPX_NATIVE_COLLABORATION_CONFIG !== undefined;
}

async function read(request: Request, holdingSlug: string, path: string): Promise<unknown> {
  const config = configuration();
  const headers = new Headers({ accept: 'application/json' });
  const cookie = request.headers.get('cookie');
  if (cookie) headers.set('cookie', cookie);
  for (const name of forwardedHeaders) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  return fetchNative(config, holdingSlug, path, headers);
}

async function fetchNative(config: NativeInboxConfiguration, holdingSlug: string, path: string, headers: Headers): Promise<unknown> {
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/inbox${path}`,
      { method: 'GET', headers, cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  return unpack(response);
}

async function unpack(response: Response): Promise<unknown> {
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_COLLABORATION_FAILED',
      payload?.error?.message ?? 'The Collaboration request failed.',
    );
  return payload?.data;
}

const forwardedHeaders = [
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
] as const;

/** Server-component variant of the forwarded header set: the screen has no
 * Request object, so it reads the incoming headers and cookies directly.
 * Same names, same omission-when-absent rule as {@link read}. */
async function ssrHeaders(): Promise<Headers> {
  const outgoing = new Headers({ accept: 'application/json' });
  const incoming = await headers();
  const cookie = (await cookies()).toString();
  if (cookie) outgoing.set('cookie', cookie);
  for (const name of forwardedHeaders) {
    const value = incoming.get(name);
    if (value) outgoing.set(name, value);
  }
  return outgoing;
}

/** SSR variant of the native notifications page read, for server components
 * without a Request object. Same path, same forwarding, same errors. */
export async function nativeInboxNotificationsSSR(holdingSlug: string, search: string): Promise<NativeInboxNotificationsPage> {
  const config = configuration();
  return asPage(await fetchNative(config, holdingSlug, `/notifications${search}`, await ssrHeaders()));
}

/** SSR variant of the native summary read, for server components without a
 * Request object. Same path, same forwarding, same errors. */
export async function nativeInboxSummarySSR(holdingSlug: string): Promise<NativeInboxSummary> {
  const config = configuration();
  return asSummary(await fetchNative(config, holdingSlug, '/summary', await ssrHeaders()));
}

export type NativeInboxNotificationsPage = {
  items: InboxItem[];
  nextCursor: string | null;
  hasMore: boolean;
};

export type NativeInboxSummary = {
  unread: number;
  categories: Record<string, number>;
};

const inboxCategories = ["work", "calendar", "mention", "notice", "system"] as const;

function failed(): ApiError {
  return new ApiError(503, 'NATIVE_COLLABORATION_FAILED', 'The Collaboration request failed.');
}

function asPage(data: unknown): NativeInboxNotificationsPage {
  if (!data || typeof data !== "object") throw failed();
  const { items, nextCursor, hasMore } = data as { items?: unknown; nextCursor?: unknown; hasMore?: unknown };
  if (!Array.isArray(items)) throw failed();
  for (const item of items) {
    if (!item || typeof item !== "object") throw failed();
    const row = item as Record<string, unknown>;
    if (typeof row.id !== "string" || typeof row.eventId !== "string" || typeof row.message !== "string"
      || typeof row.createdAt !== "number" || typeof row.sourceType !== "string" || typeof row.sourceId !== "string"
      || (typeof row.category !== "string" || !(inboxCategories as readonly string[]).includes(row.category))
      || (row.href !== null && typeof row.href !== "string")
      || (row.readAt !== null && typeof row.readAt !== "number")
      || (row.archivedAt !== null && typeof row.archivedAt !== "number")) throw failed();
  }
  if (nextCursor !== null && typeof nextCursor !== "string") throw failed();
  return { items: items as InboxItem[], nextCursor, hasMore: hasMore === true };
}

function asSummary(data: unknown): NativeInboxSummary {
  if (!data || typeof data !== "object") throw failed();
  const { unread, categories } = data as { unread?: unknown; categories?: unknown };
  if (typeof unread !== "number" || !categories || typeof categories !== "object") throw failed();
  return { unread, categories: categories as Record<string, number> };
}

export function nativeInboxSummary(request: Request, holdingSlug: string) {
  return read(request, holdingSlug, '/summary');
}

export function nativeInboxPeople(request: Request, holdingSlug: string, query: string) {
  return read(request, holdingSlug, `/people?q=${encodeURIComponent(query)}`);
}

export function nativeInboxNotices(request: Request, holdingSlug: string) {
  return read(request, holdingSlug, '/notices');
}

export function nativeInboxNotifications(request: Request, holdingSlug: string, search: string) {
  return read(request, holdingSlug, `/notifications${search}`);
}
