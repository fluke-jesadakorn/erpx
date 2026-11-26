import 'server-only';
import {
  NativeCommerceOrdersError,
  directHttp,
  nativeCommerceOrdersConfiguration,
  type NativeCommerceOrdersTransport,
} from '@erpx/feature-commerce/public/native-commerce-orders';

/** Exact `data` payload of the native Commerce return-line read. */
export type NativeCommerceReturnLine = Readonly<{
  id: string;
  fulfillment_id: string;
  order_line_id: string;
  quantity_milli: number;
  value_minor: number;
  subtotal_minor: number;
  tax_minor: number;
  product_id: string;
  item_type: string;
  description: string;
  sku: string;
  returned_milli: number;
}>;

const KEYS = [
  'id', 'fulfillment_id', 'order_line_id', 'quantity_milli', 'value_minor',
  'subtotal_minor', 'tax_minor', 'product_id', 'item_type', 'description',
  'sku', 'returned_milli',
] as const;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;

function line(value: unknown): value is NativeCommerceReturnLine {
  if (!exact(value, KEYS)) return false;
  return (
    identifier(value.id) &&
    identifier(value.fulfillment_id) &&
    identifier(value.order_line_id) &&
    integer(value.quantity_milli) &&
    integer(value.value_minor) &&
    integer(value.subtotal_minor) &&
    integer(value.tax_minor) &&
    identifier(value.product_id) &&
    text(value.item_type, 30) &&
    text(value.description, 2000) &&
    text(value.sku, 64) &&
    integer(value.returned_milli)
  );
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_RESPONSE_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

/**
 * Thin, fail-closed transport for the native owner route. It carries no SQL,
 * no fallback and no payload shaping; a malformed or absent native response
 * fails rather than reaching a retained TypeScript reader.
 */
export function nativeCommerceReturnLineReader(
  configurationRaw: string,
  transport: NativeCommerceOrdersTransport = directHttp,
) {
  const raw = nativeCommerceOrdersConfiguration(configurationRaw);
  const base = new URL(raw.privateBaseUrl);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port)
    throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_CONFIGURATION_INVALID');
  const holdings = raw.holdings.map((holding) => ({ ...holding, origin: new URL(holding.origin) }));
  if (holdings.some((holding) => holding.origin.protocol !== 'https:'))
    throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_CONFIGURATION_INVALID');
  return {
    async read(
      incoming: Pick<Headers, 'get'>,
      holdingSlug: string,
      companyId: string,
      orderId: string,
      signal?: AbortSignal,
    ): Promise<NativeCommerceReturnLine[]> {
      const host = incoming.get('host');
      const holding = host ? holdings.find((candidate) => candidate.origin.host === host) : undefined;
      if (!holding || holding.holdingSlug !== holdingSlug || !identifier(companyId) || !identifier(orderId))
        throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_AUTHORITY_INVALID');
      const controller = new AbortController();
      const cancel = () => controller.abort();
      signal?.addEventListener('abort', cancel, { once: true });
      if (signal?.aborted) controller.abort();
      const timer = setTimeout(cancel, raw.timeoutMs);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/commerce/return-lines`,
        base,
      );
      const response = await transport(
        'GET',
        url,
        new Headers({
          host: host!,
          accept: 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': companyId,
          'x-erpx-context-record': orderId,
        }),
        undefined,
        controller.signal,
      ).finally(() => {
        clearTimeout(timer);
        signal?.removeEventListener('abort', cancel);
      });
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
        void response.body?.cancel().catch(() => undefined);
        throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_RESPONSE_INVALID');
      }
      const data = await boundedBytes(response, 2 * 1024 * 1024);
      let value: unknown;
      try {
        value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
      } catch {
        throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_RESPONSE_INVALID');
      }
      if (!exact(value, ['data']) || !Array.isArray(value.data) || !value.data.every(line))
        throw new NativeCommerceOrdersError('NATIVE_COMMERCE_RETURN_LINE_RESPONSE_INVALID');
      return value.data as NativeCommerceReturnLine[];
    },
  };
}
