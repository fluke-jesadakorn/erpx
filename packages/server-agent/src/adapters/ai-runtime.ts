import 'server-only';
import { aiGatewayBootstrapSources } from '@erpx/server-manage_erpx/public/ai-gateway-sources';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import { companyAuthority } from '@erpx/server-operations/public/staff-policy';
import type { BetaEnvironment } from '@erpx/server-manage_erpx/public/beta-config';
import { assertion } from '@erpx/server-workflow/public/command-assertions';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  openAiKey,
  type AiEndpoint,
} from '@erpx/server-agent/adapters/ai-transport';
import {
  isThaiHotelAiMockEnabled,
  thaiHotelAiMockId,
} from '@erpx/server-runtime/public/simulation-only-adapters';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type {
  AiConnectionView,
  AiProviderOverview,
  AiProviderSnapshot,
  Connection,
  Revision,
  Selection,
} from '@erpx/server-agent/domain/providers';
import type { AiEndpoint as CoreEndpoint } from '@erpx/server-agent/domain/transport';

const roles = ['holding_owner', 'it_admin'];

async function exposed<T>(action: () => Promise<T>): Promise<T> {
  try {
    return await action();
  } catch (error) {
    if (error instanceof BusinessRuleError)
      throw new ApiError(error.status, error.code, error.message);
    throw error;
  }
}
function exposedSync<T>(action: () => T): T {
  try {
    return action();
  } catch (error) {
    if (error instanceof BusinessRuleError)
      throw new ApiError(error.status, error.code, error.message);
    throw error;
  }
}
function nativeEndpoint(db: SqlDatabase, endpoint: CoreEndpoint): AiEndpoint {
  const { gateway, ...rest } = endpoint;
  return gateway ? { ...rest, gateway: { ...gateway, db } } : rest;
}
async function requireAiAdmin(db: SqlDatabase, actor: StaffActor) {
  if (!(await companyAuthority(db, actor, roles)))
    throw new BusinessRuleError(
      403,
      'AI_ADMIN_REQUIRED',
      'A company owner or IT administrator is required.',
    );
}
async function connection(db: SqlDatabase, companyId: string, id: string) {
  const row = await db
    .prepare(
      'SELECT * FROM ai_connections WHERE id=? AND company_id=? AND deleted_at IS NULL',
    )
    .bind(id, companyId)
    .first<Connection>();
  if (!row)
    throw new BusinessRuleError(
      404,
      'AI_CONNECTION_NOT_FOUND',
      'AI connection not found in this company.',
    );
  return row;
}
async function revision(
  db: SqlDatabase,
  row: Connection,
  active = false,
) {
  const result = await db
    .prepare(
      'SELECT * FROM ai_connection_revisions WHERE connection_id=? AND revision=?',
    )
    .bind(row.id, active ? row.active_revision : row.draft_revision)
    .first<Revision>();
  if (!result)
    throw new BusinessRuleError(
      409,
      'AI_REVISION_UNAVAILABLE',
      'This connection has no usable configuration revision.',
    );
  return result;
}
async function currentSelection(db: SqlDatabase, companyId: string) {
  return db
    .prepare('SELECT * FROM ai_company_settings WHERE company_id=?')
    .bind(companyId)
    .first<Selection>();
}
/**
 * Full retained Administration overview read. The feature-admin route reads
 * this payload from the native owner; in-process presentation and workspace
 * assistant callers retain the same shape here.
 */
export async function aiProviderOverview(
  db: SqlDatabase,
  actor: StaffActor,
  env: BetaEnvironment,
): Promise<AiProviderOverview> {
  return exposed(async () => {
    await requireAiAdmin(db, actor);
    const rows = (
      await db
        .prepare(
          'SELECT * FROM ai_connections WHERE company_id=? AND deleted_at IS NULL ORDER BY created_at,id',
        )
        .bind(actor.companyId)
        .all<Connection>()
    ).results;
    const connections: AiConnectionView[] = [];
    for (const row of rows) {
      const rev = await revision(db, row);
      const { credential_json, ...safe } = rev;
      const active = row.active_revision ? await revision(db, row, true) : null;
      connections.push({
        ...row,
        ...safe,
        hasKey: !!credential_json,
        active: active
          ? {
              base_url: active.base_url,
              text_protocol: active.text_protocol,
              image_protocol: active.image_protocol,
              text_model: active.text_model,
              image_model: active.image_model,
            }
          : null,
      });
    }
    const selection = (await currentSelection(db, actor.companyId)) ?? {
      company_id: actor.companyId,
      version: 0,
      mode: process.env.NODE_ENV === 'production' ? 'disabled' : 'deployment',
      text_connection_id: null,
      image_connection_id: null,
      image_mode: 'default',
      home_ai_name: 'AI',
    };
    const usage = (
      await db
        .prepare(
          "SELECT connection_id,model,COUNT(*) requests,SUM(input_tokens) input_tokens,SUM(output_tokens) output_tokens,SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END) failures FROM ai_provider_usage WHERE company_id=? AND created_at>=? GROUP BY connection_id,model",
        )
        .bind(actor.companyId, Date.now() - 30 * 86400000)
        .all<AiProviderOverview['usage'][number]>()
    ).results;
    return {
      connections,
      selection,
      deploymentAvailable: Boolean(env.AI_BASE_URL && env.AI_MODEL && env.AI_API_KEY),
      keyStorageReady: Boolean(env.AI_CREDENTIAL_KEYS),
      usage,
    };
  });
}
async function gatewayEndpoint(
  db: SqlDatabase,
  companyId: string,
  target: CoreEndpoint,
  connectionId: string | null,
  holdingId?: string,
): Promise<CoreEndpoint> {
  const holding = await aiGatewayBootstrapSources(db).binding();
  if (holding && holdingId && holding.holding_id !== holdingId)
    throw new BusinessRuleError(
      403,
      'AI_GATEWAY_SCOPE',
      'The AI billing scope does not match this Holding.',
    );
  const scopedHoldingId = holdingId ?? holding?.holding_id;
  if (!scopedHoldingId)
    throw new BusinessRuleError(
      503,
      'AI_GATEWAY_SCOPE',
      "The Holding's AI billing scope is unavailable.",
    );
  return {
    ...target,
    gateway: {
      holdingId: scopedHoldingId,
      companyId,
      branchId: null,
      connectionId,
      fundingSource: connectionId ? 'byok' : 'erpx',
      feature: '',
      requestId: '',
    },
  };
}
async function endpoint(
  env: BetaEnvironment,
  companyId: string,
  row: Connection,
  rev: Revision,
  kind: 'text' | 'image',
): Promise<CoreEndpoint> {
  if (rev.auth_mode === 'bearer' && !rev.credential_json)
    throw new BusinessRuleError(
      422,
      'AI_KEY_REQUIRED',
      'Enter an API key before testing this Bearer connection.',
    );
  return {
    baseUrl: rev.base_url,
    protocol: kind === 'image' ? rev.image_protocol : rev.text_protocol,
    apiKey:
      rev.auth_mode === 'bearer' && rev.credential_json
        ? await openAiKey(env, companyId, row.id, rev.credential_json)
        : null,
    model: kind === 'image' ? (rev.image_model ?? rev.text_model) : rev.text_model,
  };
}
export async function resolveAiProvider(
  db: SqlDatabase,
  companyId: string,
  env: BetaEnvironment,
  kind: 'text' | 'image',
  holdingSlug?: string,
  holdingId?: string,
) {
  const result = await exposed(async () => {
    const settings = await currentSelection(db, companyId);
    if (isThaiHotelAiMockEnabled(holdingSlug)) {
      if (
        settings?.mode === 'disabled' ||
        (kind === 'image' && settings?.image_mode === 'disabled')
      )
        throw new BusinessRuleError(
          503,
          'AI_DISABLED',
          'AI is disabled for this capability. Continue manually.',
        );
      return {
        endpoint: {
          baseUrl: 'simulation://thai-hotel-v1',
          model: thaiHotelAiMockId,
          apiKey: null,
        },
        snapshot: {
          companyId,
          connectionId: null,
          revision: 1,
          settingsVersion: settings?.version ?? 0,
          model: thaiHotelAiMockId,
          kind,
          deploymentDigest: `simulation:${thaiHotelAiMockId}`,
        } satisfies AiProviderSnapshot,
      };
    }
    if (
      process.env.NODE_ENV === 'production' &&
      (!settings || settings.mode === 'disabled')
    )
      throw new BusinessRuleError(
        503,
        'AI_PILOT_OPT_IN_REQUIRED',
        'AI is not enabled for this company. Continue manually or ask an administrator to enable a provider.',
      );
    if (
      settings?.mode === 'disabled' ||
      (kind === 'image' && settings?.image_mode === 'disabled')
    )
      throw new BusinessRuleError(
        503,
        'AI_DISABLED',
        'AI is disabled for this capability. Continue manually.',
      );
    if (!settings || settings.mode === 'deployment') {
      if (!env.AI_BASE_URL || !env.AI_MODEL || !env.AI_API_KEY)
        throw new BusinessRuleError(
          503,
          'AI_UNAVAILABLE',
          'No AI connection is configured. Continue manually or ask an owner or IT administrator to configure AI providers.',
        );
      if (kind === 'image')
        throw new BusinessRuleError(
          503,
          'AI_IMAGE_TEST_REQUIRED',
          'Configure and test an image model in AI providers to read documents.',
        );
      return {
        endpoint: await gatewayEndpoint(
          db,
          companyId,
          {
            baseUrl: env.AI_BASE_URL,
            model: env.AI_MODEL,
            apiKey: env.AI_API_KEY,
          },
          null,
          holdingId,
        ),
        snapshot: {
          companyId,
          connectionId: null,
          revision: 0,
          settingsVersion: settings?.version ?? 0,
          model: env.AI_MODEL,
          kind,
          deploymentDigest: await sha256(
            `${env.AI_BASE_URL}:${env.AI_MODEL}:${env.AI_API_KEY}`,
          ),
        } satisfies AiProviderSnapshot,
      };
    }
    const id =
      kind === 'image' && settings.image_mode === 'custom'
        ? settings.image_connection_id
        : settings.text_connection_id;
    if (!id)
      throw new BusinessRuleError(
        503,
        'AI_UNAVAILABLE',
        'Choose an AI connection in Administration.',
      );
    const row = await connection(db, companyId, id);
    const rev = await revision(db, row, true);
    if (
      row.disabled ||
      !rev.text_verified ||
      (kind === 'image' && !rev.image_verified)
    )
      throw new BusinessRuleError(
        503,
        'AI_CAPABILITY_UNAVAILABLE',
        'The selected connection is disabled or has not passed the required capability test.',
      );
    const target = await endpoint(env, companyId, row, rev, kind);
    return {
      endpoint: await gatewayEndpoint(db, companyId, target, id, holdingId),
      snapshot: {
        companyId,
        connectionId: id,
        revision: rev.revision,
        settingsVersion: settings.version,
        model: target.model,
        kind,
      } satisfies AiProviderSnapshot,
    };
  });
  return { ...result, endpoint: nativeEndpoint(db, result.endpoint) };
}
export function aiSnapshotGuard(
  db: SqlDatabase,
  key: string,
  snapshot: AiProviderSnapshot,
  label = 'ai-provider-revision',
) {
  return exposedSync(() => {
    const simulationSnapshot =
      snapshot.deploymentDigest?.startsWith('simulation:') ?? false;
    const settings =
      'COALESCE((SELECT version FROM ai_company_settings WHERE company_id=?),0)=?';
    return assertion(
      db,
      key,
      label,
      `${settings}${snapshot.connectionId ? ' AND EXISTS(SELECT 1 FROM ai_connections c JOIN ai_connection_revisions r ON r.connection_id=c.id AND r.revision=c.active_revision WHERE c.id=? AND c.company_id=? AND c.active_revision=? AND c.disabled=0 AND c.deleted_at IS NULL AND r.text_verified=1 AND (?=\'text\' OR r.image_verified=1))' : simulationSnapshot ? '' : " AND COALESCE((SELECT mode FROM ai_company_settings WHERE company_id=?),'deployment')='deployment'"}`,
      [
        snapshot.companyId,
        snapshot.settingsVersion,
        ...(snapshot.connectionId
          ? [snapshot.connectionId, snapshot.companyId, snapshot.revision, snapshot.kind]
          : simulationSnapshot
            ? []
            : [snapshot.companyId]),
      ],
    );
  });
}
export function aiUsageStatement(
  db: SqlDatabase,
  id: string,
  snapshot: AiProviderSnapshot,
  feature: string,
  status: 'succeeded' | 'failed',
  inputTokens = 0,
  outputTokens = 0,
  branchId: string | null = null,
) {
  const fundingSource = snapshot.deploymentDigest?.startsWith('simulation:')
    ? 'simulation'
    : snapshot.connectionId
      ? 'byok'
      : snapshot.deploymentDigest
        ? 'erpx'
        : 'unknown';
  return db
    .prepare(
      'INSERT INTO ai_provider_usage(id,company_id,connection_id,revision,model,feature,status,input_tokens,output_tokens,created_at,branch_id,funding_source) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING',
    )
    .bind(
      id,
      snapshot.companyId,
      snapshot.connectionId,
      snapshot.revision,
      snapshot.model,
      feature,
      status,
      inputTokens,
      outputTokens,
      Date.now(),
      branchId,
      fundingSource,
    );
}
export type { AiConnectionView, AiProviderOverview, AiProviderSnapshot } from '@erpx/server-agent/domain/providers';
