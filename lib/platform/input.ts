export const PLATFORM_ID_MAX_LENGTH = 200;
export const PLATFORM_SEARCH_MAX_LENGTH = 120;
export const PLATFORM_TEXT_MAX_LENGTH = 5000;

const SAFE_ERROR_MESSAGES = new Set([
  'support ticket not found',
  'ticket/customer scope mismatch',
  'closed tickets cannot receive support sessions',
  'impersonation requires an explicitly represented tenant user',
  'support session is no longer active',
  'support session not found',
  'customer approval scope mismatch',
  'support session is expired or revoked',
  'support session is not awaiting approval',
  'support capability is not active',
  'support capability does not grant the requested permission',
  'support capability does not cover the requested module',
  'support scope identifiers must be UUIDs',
  'support scope exceeds the ticket company',
  'support scope exceeds the ticket branch',
  'support scope is outside the customer',
  'support action is outside the approved branch scope',
  'support action is outside the approved company scope',
  'support action scope identifiers must be UUIDs',
  'support action is outside the customer scope',
  'parent assignment not found',
  'child assignment must start within an active parent assignment',
  'child scope exceeds parent assignment scope',
  'child assignment cannot outlive its parent',
  'assignment not found',
  'renewed assignment cannot outlive its parent',
]);

export function boundedText(value: unknown, maxLength = PLATFORM_TEXT_MAX_LENGTH): string | null {
  if (typeof value !== 'string') return null;
  const normalized = value.trim();
  return normalized.length > 0 && normalized.length <= maxLength ? normalized : null;
}

export function boundedOptionalText(value: unknown, maxLength = PLATFORM_ID_MAX_LENGTH): string | null {
  if (value == null || value === '') return null;
  return boundedText(value, maxLength);
}

export function objectBody(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

export function boundedStringArray(value: unknown, maxItems: number, maxItemLength = PLATFORM_ID_MAX_LENGTH): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .slice(0, maxItems)
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim())
    .filter((item) => item.length > 0 && item.length <= maxItemLength);
}

export function boundedUuidArray(value: unknown, maxItems = 100): string[] {
  return boundedStringArray(value, maxItems, 64).filter(isUuid);
}

export function boundedSearch(value: string | null): string | undefined {
  if (!value) return undefined;
  const normalized = value.trim().replace(/\s+/g, ' ');
  return normalized.length > 0 && normalized.length <= PLATFORM_SEARCH_MAX_LENGTH ? normalized : undefined;
}

export function isUuid(value: unknown): value is string {
  return typeof value === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value.trim());
}

export function safeApiError(error: unknown, fallback: string): string {
  if (!(error instanceof Error)) return fallback;
  if (SAFE_ERROR_MESSAGES.has(error.message)) return error.message;
  if (error.message.startsWith('support session duration exceeds the ')) return error.message;
  if (error.message.startsWith('assignment exceeds the ')) return error.message;
  return fallback;
}
