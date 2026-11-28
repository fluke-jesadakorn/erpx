import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { GpoRule, GpoRulePreview, GpoRulesOverview } from '@erpx/organization-ui/lib/gpo-rules';
import type { EffectiveRuleRow, GpoRuleRow } from '@erpx/identity-access-server/public/gpo-contract';

type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Configuration = Readonly<{
  base: URL;
  holdings: ReadonlyMap<string, Holding>;
  timeout: number;
}>;

const CONTEXT_HEADERS = [
  'cookie',
  'origin',
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
] as const;

const COMMANDS = new Set([
  'preview',
  'publish',
  'unmap-preview',
  'unmap',
  'retry',
  'retry-config',
  'not-required',
]);

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configurationInvalid(): ApiError {
  return new ApiError(
    503,
    'NATIVE_POSITION_GPO_CONFIGURATION_INVALID',
    'Position GPO transport is misconfigured.',
  );
}

function unavailable(): ApiError {
  return new ApiError(
    503,
    'NATIVE_POSITION_GPO_UNAVAILABLE',
    'The native Position GPO service is unavailable.',
  );
}

function configuration(): Configuration {
  const raw = process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  if (raw === undefined) throw unavailable();
  let value: {
    privateBaseUrl?: unknown;
    holdings?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw configurationInvalid();
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    throw configurationInvalid();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    throw configurationInvalid();
  const holdings = new Map<string, Holding>();
  for (const item of value.holdings as unknown[]) {
    if (!item || typeof item !== 'object') throw configurationInvalid();
    const holding = item as Holding;
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      throw configurationInvalid();
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      typeof holding.holdingId !== 'string' ||
      !/^[A-Za-z0-9_.:-]{1,200}$/.test(holding.holdingId) ||
      typeof holding.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      throw configurationInvalid();
    holdings.set(origin.host, holding);
  }
  return { base, holdings, timeout: value.timeoutMs as number };
}

/** The native owner service hosts the Position GPO ingress when configured. */
export function nativePositionGpoAvailable(): boolean {
  return process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG !== undefined;
}

function holdingFor(
  source: Headers,
  holdingSlug: string,
  configuration: Configuration,
): Holding {
  const origin = source.get('origin');
  let host = source.get('host')?.split(':')[0];
  if (!host && origin) {
    try {
      host = new URL(origin).host.split(':')[0];
    } catch {
      throw unavailable();
    }
  }
  const holding = host ? configuration.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    throw new ApiError(403, 'NATIVE_POSITION_GPO_AUTHORITY_INVALID', 'Use the registered Holding host.');
  return holding;
}

function holdingById(holdingId: string, configuration: Configuration): Holding {
  for (const holding of configuration.holdings.values())
    if (holding.holdingId === holdingId) return holding;
  throw unavailable();
}

function requestHeaders(source: Headers, holding: Holding, idempotencyKey?: string): Headers {
  const headers = new Headers({
    host: new URL(holding.origin).host,
    accept: 'application/json',
  });
  for (const name of CONTEXT_HEADERS) {
    const value = source.get(name);
    if (value) headers.set(name, value);
  }
  if (idempotencyKey) {
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', idempotencyKey);
  }
  return headers;
}

async function requestHolding(
  source: Headers,
  holding: Holding,
  path: string,
  method: 'GET' | 'POST',
  body?: unknown,
  key?: string,
): Promise<unknown> {
  const config = configuration();
  let response: Response;
  try {
    response = await fetch(
      `${config.base.origin}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/technology/position-gpo${path}`,
      {
        method,
        headers: requestHeaders(source, holding, key),
        body: body === undefined ? undefined : JSON.stringify(body),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeout),
      },
    );
  } catch {
    throw unavailable();
  }
  return payload(response);
}

async function payload(response: Response): Promise<unknown> {
  const value = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'NATIVE_POSITION_GPO_FAILED',
      value?.error?.message ?? 'The Position GPO request failed.',
    );
  return value?.data;
}

/** One read: the Position GPO overview for the current Holding request. */
export function nativePositionGpoOverview(
  request: Request,
  holdingSlug: string,
): Promise<unknown> {
  const config = configuration();
  return requestHolding(request.headers, holdingFor(request.headers, holdingSlug, config), '', 'GET');
}

/** Browser-header variant for presenters that already own the request headers. */
export function nativePositionGpoOverviewFromHeaders(
  source: Headers,
  holdingSlug: string,
): Promise<unknown> {
  const config = configuration();
  return requestHolding(source, holdingFor(source, holdingSlug, config), '', 'GET');
}

/** Server-side transport by the selected Holding identity (no browser slug). */
export function nativePositionGpoOverviewForHolding(
  source: Headers,
  holdingId: string,
): Promise<unknown> {
  return requestHolding(source, holdingById(holdingId, configuration()), '', 'GET');
}

/** One reviewed command: preview, publish, unmap, retry, retry-config or not-required. */
export async function nativePositionGpoCommand(
  request: Request,
  holdingSlug: string,
  operation: string,
  body: unknown,
  key: string,
): Promise<unknown> {
  if (!COMMANDS.has(operation))
    throw new ApiError(404, 'NOT_FOUND', 'Position GPO operation not found.');
  const config = configuration();
  return requestHolding(
    request.headers,
    holdingFor(request.headers, holdingSlug, config),
    `/${operation}`,
    'POST',
    body,
    key,
  );
}

/** Project the native overview into the owner's read ports. */
export async function nativeGpoRules(
  source: Headers,
  holdingId: string,
  companyId: string,
): Promise<GpoRuleRow[]> {
  const data = (await nativePositionGpoOverviewForHolding(source, holdingId)) as GpoRulesOverview;
  return data.rules.map(rule => projectRule(rule, companyId));
}

/** Project the native effective view into the owner's read ports. */
export async function nativeEffectiveGpoRules(
  source: Headers,
  holdingId: string,
  companyId: string,
): Promise<EffectiveRuleRow[]> {
  const data = (await nativePositionGpoOverviewForHolding(source, holdingId)) as GpoRulesOverview;
  return Object.entries(data.effectiveByPosition).flatMap(([positionId, rules]) =>
    rules.map(rule => ({ ...projectRule(rule, companyId), position_id: positionId })),
  );
}

function projectRule(rule: GpoRule, companyId: string): GpoRuleRow {
  return {
    id: rule.id,
    company_id: companyId,
    target_kind: rule.target.kind,
    target_id: rule.target.id,
    target_name: rule.targetName,
    version: rule.version,
    revision: rule.revision,
    connection_id: rule.connectionId,
    gpo_id: rule.gpoId,
    user_filter_group_id: rule.userFilterGroupId,
    computer_target_ou_id: rule.computerTargetOuId,
    active: rule.active,
    directory_config_status: rule.directoryStatus,
    directory_config_error: rule.directoryError,
    legacy_position_id: rule.legacyPositionId,
  };
}

/**
 * The organization technology workspace keeps its gpo-rules wire contract; the
 * transport translates it to the native Position GPO commands and projects the
 * reviewed result back. No TypeScript business authority remains.
 */
type RuleInput = {
  id?: string;
  target: { kind: string; id: string };
  expectedVersion: number;
  connectionId: string;
  gpoId: string;
  userFilterGroupId: string;
  computerTargetOuId: string;
  reason: string;
};
type RemovalInput = { id: string; expectedVersion: number; reason: string };

function rulePosition(rule: RuleInput): string {
  if (rule.target?.kind !== 'position' || typeof rule.target.id !== 'string' || !rule.target.id)
    throw new ApiError(422, 'GPO_TARGET_INVALID', 'GPO rules can only target a Position.');
  return rule.target.id;
}

function nativeRuleInput(rule: RuleInput) {
  return {
    positionId: rulePosition(rule),
    expectedVersion: rule.expectedVersion,
    connectionId: rule.connectionId,
    gpoId: rule.gpoId,
    userFilterGroupId: rule.userFilterGroupId,
    computerTargetOuId: rule.computerTargetOuId,
    reason: rule.reason,
  };
}

function nativePreview(raw: Record<string, unknown>): RuleInput {
  if (!raw || typeof raw !== 'object') throw new ApiError(422, 'GPO_REQUEST_INVALID', 'Invalid GPO rule input.');
  return raw as unknown as RuleInput;
}

function projectPreview(value: Record<string, unknown>, input: RuleInput): GpoRulePreview {
  const position = (value.position ?? {}) as { id?: string; title?: string };
  const holders = (Array.isArray(value.holders) ? value.holders : []) as {
    person_id?: string; engagement_id?: string; name?: string; account_id?: string;
  }[];
  const changes = (value.changes ?? {}) as {
    membersToAdd?: unknown[]; membersToRemoveFromPreviousGroup?: unknown[];
  };
  return {
    token: String(value.token ?? ''),
    expiresAt: Date.now() + 600_000,
    target: { kind: 'position', id: rulePosition(input) },
    targetName: String(position.title ?? ''),
    positions: position.id ? [{ id: position.id, title: String(position.title ?? '') }] : [],
    holders: holders.map(holder => ({
      personId: String(holder.person_id ?? ''),
      engagementId: String(holder.engagement_id ?? ''),
      name: String(holder.name ?? ''),
      accountIds: holder.account_id ? [holder.account_id] : [],
    })),
    changes: {
      add: (changes.membersToAdd ?? []).map(String),
      remove: (changes.membersToRemoveFromPreviousGroup ?? []).map(String),
      retained: (Array.isArray(value.sharedMappings) ? value.sharedMappings : []).map(String),
    },
    inheritedByFuturePositions: false,
    endpointStatus: 'unconfirmed',
  };
}

export async function nativeGpoRulesOverview(request: Request, holdingSlug: string): Promise<GpoRulesOverview> {
  return (await nativePositionGpoOverview(request, holdingSlug)) as GpoRulesOverview;
}

export async function nativePreviewGpoRule(request: Request, holdingSlug: string, raw: unknown): Promise<GpoRulePreview> {
  const input = nativePreview(raw as Record<string, unknown>);
  const value = (await nativePositionGpoCommand(request, holdingSlug, 'preview', nativeRuleInput(input), '')) as Record<string, unknown>;
  return projectPreview(value, input);
}

export async function nativePublishGpoRule(request: Request, holdingSlug: string, raw: unknown, key: string): Promise<unknown> {
  const input = nativePreview(raw as Record<string, unknown>);
  const { previewToken, ...rest } = input as RuleInput & { previewToken?: string };
  if (typeof previewToken !== 'string')
    throw new ApiError(422, 'GPO_PREVIEW_REQUIRED', 'A reviewed preview token is required.');
  return nativePositionGpoCommand(
    request,
    holdingSlug,
    'publish',
    { ...nativeRuleInput(rest), previewToken },
    key,
  );
}

async function rulePositionById(request: Request, holdingSlug: string, id: string): Promise<string> {
  const overview = (await nativePositionGpoOverview(request, holdingSlug)) as GpoRulesOverview;
  const rule = overview.rules.find(candidate => candidate.id === id);
  if (!rule) throw new ApiError(409, 'GPO_RULE_CHANGED', 'Reload the current GPO rule before reviewing this change.');
  return rulePosition({ target: rule.target } as RuleInput);
}

function removalInput(raw: unknown): RemovalInput {
  if (!raw || typeof raw !== 'object') throw new ApiError(422, 'GPO_REQUEST_INVALID', 'Invalid GPO removal input.');
  return raw as RemovalInput;
}

export async function nativePreviewRemoveGpoRule(request: Request, holdingSlug: string, raw: unknown): Promise<GpoRulePreview> {
  const input = removalInput(raw);
  const positionId = await rulePositionById(request, holdingSlug, input.id);
  const removal = { positionId, expectedVersion: input.expectedVersion, reason: input.reason };
  const value = (await nativePositionGpoCommand(request, holdingSlug, 'unmap-preview', removal, '')) as Record<string, unknown>;
  return projectPreview(value, { target: { kind: 'position', id: positionId }, expectedVersion: input.expectedVersion, connectionId: '', gpoId: '', userFilterGroupId: '', computerTargetOuId: '', reason: input.reason });
}

export async function nativeRemoveGpoRule(request: Request, holdingSlug: string, raw: unknown, key: string): Promise<unknown> {
  const input = removalInput(raw);
  const { previewToken } = input as RemovalInput & { previewToken?: string };
  if (typeof previewToken !== 'string')
    throw new ApiError(422, 'GPO_PREVIEW_REQUIRED', 'A reviewed preview token is required.');
  const positionId = await rulePositionById(request, holdingSlug, input.id);
  return nativePositionGpoCommand(
    request,
    holdingSlug,
    'unmap',
    { positionId, expectedVersion: input.expectedVersion, reason: input.reason, previewToken },
    key,
  );
}

export async function nativeRetryGpoRuleConfig(request: Request, holdingSlug: string, raw: unknown, key: string): Promise<unknown> {
  const input = removalInput(raw);
  const positionId = await rulePositionById(request, holdingSlug, input.id);
  return nativePositionGpoCommand(
    request,
    holdingSlug,
    'retry-config',
    { positionId, expectedVersion: input.expectedVersion, reason: input.reason },
    key,
  );
}

export async function nativeMarkPositionGpoNotRequired(request: Request, holdingSlug: string, raw: unknown, key: string): Promise<unknown> {
  return nativePositionGpoCommand(request, holdingSlug, 'not-required', raw, key);
}
