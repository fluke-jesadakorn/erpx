import { aiPresets, supportedModelProtocol, supportedModels, unsupportedModelMessage, presetForUrl, type AiProtocol, type AiPreset } from "@/lib/ai/providers";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { z } from "zod";
import { sha256 } from "./crypto";
import type { StaffActor } from "./staff-policy";
import { companyAuthority, authorityGuard } from "./staff-policy";
import type { BetaEnvironment } from "./beta-config";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import {
  aiBaseUrl,
  generateProviderJson,
  openAiKey,
  providerRequest,
  sealAiKey,
  type AiEndpoint,
} from "./ai-transport";

export const aiConnectionSchema = z.object({
  expectedVersion: z.number().int().nonnegative().default(0),
  name: z.string().trim().min(2).max(100),
  baseUrl: z.string().trim().max(500),
  authMode: z.enum(["bearer", "none"]).default("bearer"),
  apiKey: z.string().max(8000).optional(),
  removeKey: z.boolean().default(false),
  providerPreset: z.enum(["custom", "opencode-zen", "opencode-go"]).default("custom"),
  textModel: z.string().trim().min(1).max(200),
  imageModel: z.string().trim().max(200).default(""),
});
export const aiVersionSchema = z.object({
  expectedVersion: z.number().int().positive(),
});
export const aiSelectionSchema = z.object({
  expectedVersion: z.number().int().nonnegative(),
  mode: z.enum(["deployment", "custom", "disabled"]),
  textConnectionId: z.string().nullable().default(null),
  imageMode: z.enum(["default", "custom", "disabled"]).default("default"),
  imageConnectionId: z.string().nullable().default(null),
});
type Connection = {
  id: string;
  company_id: string;
  name: string;
  version: number;
  draft_revision: number;
  active_revision: number | null;
  disabled: number;
  deleted_at: number | null;
};
type Revision = {
  connection_id: string;
  revision: number;
  base_url: string;
  auth_mode: "bearer" | "none";
  credential_json: string | null;
  provider_preset: AiPreset;
  text_protocol: AiProtocol;
  image_protocol: AiProtocol;
  text_model: string;
  image_model: string | null;
  text_verified: number;
  image_verified: number;
  tested_at: number | null;
  test_error: string | null;
};
type Selection = {
  company_id: string;
  version: number;
  mode: "deployment" | "custom" | "disabled";
  text_connection_id: string | null;
  image_connection_id: string | null;
  image_mode: "default" | "custom" | "disabled";
};
export type AiConnectionView = Connection &
  Omit<Revision, "credential_json"> & {
    hasKey: boolean;
    active: Pick<Revision, "base_url" | "text_model" | "image_model" | "text_protocol" | "image_protocol"> | null;
  };
export type AiProviderOverview = {
  connections: AiConnectionView[];
  selection: Selection;
  deploymentAvailable: boolean;
  keyStorageReady: boolean;
  usage: {
    connection_id: string | null;
    model: string;
    requests: number;
    input_tokens: number;
    output_tokens: number;
    failures: number;
  }[];
};
export type AiProviderSnapshot = {
  companyId: string;
  connectionId: string | null;
  revision: number;
  settingsVersion: number;
  model: string;
  kind: "text" | "image";
  deploymentDigest?: string;
};
const roles = ["holding_owner", "it_admin"];
export async function requireAiAdmin(db: SqlDatabase, actor: StaffActor) {
  if (!(await companyAuthority(db, actor, roles)))
    throw new ApiError(
      403,
      "AI_ADMIN_REQUIRED",
      "A company owner or IT administrator is required.",
    );
}
const guard = (db: SqlDatabase, key: string, actor: StaffActor) =>
  authorityGuard(db, key, actor, roles);
async function connection(db: SqlDatabase, companyId: string, id: string) {
  const row = await db
    .prepare(
      "SELECT * FROM ai_connections WHERE id=? AND company_id=? AND deleted_at IS NULL",
    )
    .bind(id, companyId)
    .first<Connection>();
  if (!row)
    throw new ApiError(
      404,
      "AI_CONNECTION_NOT_FOUND",
      "AI connection not found in this company.",
    );
  return row;
}
async function revision(db: SqlDatabase, row: Connection, active = false) {
  const result = await db
    .prepare(
      "SELECT * FROM ai_connection_revisions WHERE connection_id=? AND revision=?",
    )
    .bind(row.id, active ? row.active_revision : row.draft_revision)
    .first<Revision>();
  if (!result)
    throw new ApiError(
      409,
      "AI_REVISION_UNAVAILABLE",
      "This connection has no usable configuration revision.",
    );
  return result;
}
const versionGuard = (
  db: SqlDatabase,
  key: string,
  row: Connection,
  expected: number,
) =>
  assertion(
    db,
    key,
    "ai-version",
    "EXISTS(SELECT 1 FROM ai_connections WHERE id=? AND company_id=? AND version=? AND deleted_at IS NULL)",
    [row.id, row.company_id, expected],
  );
export async function aiProviderOverview(
  db: SqlDatabase,
  actor: StaffActor,
  env: BetaEnvironment,
): Promise<AiProviderOverview> {
  await requireAiAdmin(db, actor);
  const rows = (
    await db
      .prepare(
        "SELECT * FROM ai_connections WHERE company_id=? AND deleted_at IS NULL ORDER BY created_at,id",
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
  const selection = (await db
    .prepare("SELECT * FROM ai_company_settings WHERE company_id=?")
    .bind(actor.companyId)
    .first<Selection>()) ?? {
    company_id: actor.companyId,
    version: 0,
    mode: "deployment",
    text_connection_id: null,
    image_connection_id: null,
    image_mode: "default",
  };
  const usage = (
    await db
      .prepare(
        "SELECT connection_id,model,COUNT(*) requests,SUM(input_tokens) input_tokens,SUM(output_tokens) output_tokens,SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END) failures FROM ai_provider_usage WHERE company_id=? AND created_at>=? GROUP BY connection_id,model",
      )
      .bind(actor.companyId, Date.now() - 30 * 86400000)
      .all<AiProviderOverview["usage"][number]>()
  ).results;
  return {
    connections,
    selection,
    deploymentAvailable: !!(env.AI_BASE_URL && env.AI_MODEL && env.AI_API_KEY),
    keyStorageReady: !!env.AI_CREDENTIAL_KEYS,
    usage,
  };
}
export async function saveAiConnection(
  db: SqlDatabase,
  actor: StaffActor,
  env: BetaEnvironment,
  id: string | null,
  input: z.infer<typeof aiConnectionSchema>,
  key: string,
) {
  await requireAiAdmin(db, actor);
  const recordId = id ?? `ai-${key}`,
    operation = "ai.connection.save";
  const prior = await replayCommand(db, actor, key, operation, recordId, input);
  if (prior) return prior;
  const baseUrl = aiBaseUrl(input.baseUrl),
    row = id ? await connection(db, actor.companyId, id) : null,
    previous = row ? await revision(db, row) : null;
  if (input.providerPreset !== "custom" && baseUrl !== aiPresets[input.providerPreset].baseUrl)
    throw new ApiError(422, "AI_PRESET_URL", "Use the preset URL or choose Custom API.");
  let credential = previous?.credential_json ?? null;
  if (previous && baseUrl !== previous.base_url && credential && input.authMode === "bearer" && !input.removeKey && !input.apiKey?.trim())
    throw new ApiError(422, "AI_KEY_REQUIRED", "Enter a new API key when changing the provider URL.");
  const textProtocol = supportedModelProtocol(baseUrl, input.textModel);
  const imageProtocol = input.imageModel ? supportedModelProtocol(baseUrl, input.imageModel) : "chat-completions";
  if (!textProtocol || !imageProtocol)
    throw new ApiError(422, "AI_MODEL_UNSUPPORTED", unsupportedModelMessage);
  if (input.removeKey || input.authMode === "none") credential = null;
  if (input.apiKey?.trim() && input.authMode === "bearer")
    credential = await sealAiKey(
      env,
      actor.companyId,
      recordId,
      input.apiKey.trim(),
    );
  const rev = (row?.draft_revision ?? 0) + 1,
    now = Date.now(),
    version = (row?.version ?? 0) + 1;
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version },
    [
      guard(db, key, actor),
      ...(row ? [versionGuard(db, key, row, input.expectedVersion)] : []),
      row
        ? db
            .prepare(
              "UPDATE ai_connections SET name=?,draft_revision=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(input.name, rev, now, recordId)
        : db
            .prepare(
              "INSERT INTO ai_connections(id,company_id,name,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.companyId,
              input.name,
              actor.userId,
              now,
              now,
            ),
      db
        .prepare(
          "INSERT INTO ai_connection_revisions(connection_id,revision,base_url,auth_mode,credential_json,text_model,image_model,created_by,created_at,provider_preset,text_protocol,image_protocol) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          recordId,
          rev,
          baseUrl,
          input.authMode,
          credential,
          input.textModel,
          input.imageModel || null,
          actor.userId,
          now,
          input.providerPreset,
          textProtocol,
          imageProtocol,
        ),
    ],
  );
}
async function endpoint(
  env: BetaEnvironment,
  companyId: string,
  row: Connection,
  rev: Revision,
  kind: "text" | "image",
): Promise<AiEndpoint> {
  if (rev.auth_mode === "bearer" && !rev.credential_json)
    throw new ApiError(
      422,
      "AI_KEY_REQUIRED",
      "Enter an API key before testing this Bearer connection.",
    );
  return {
    baseUrl: rev.base_url,
    protocol: kind === "image" ? rev.image_protocol : rev.text_protocol,
    apiKey:
      rev.auth_mode === "bearer" && rev.credential_json
        ? await openAiKey(env, companyId, row.id, rev.credential_json)
        : null,
    model:
      kind === "image" ? (rev.image_model ?? rev.text_model) : rev.text_model,
  };
}
export async function testAiConnection(
  db: SqlDatabase,
  actor: StaffActor,
  env: BetaEnvironment,
  id: string,
  expectedVersion: number,
  key: string,
  generate = generateProviderJson,
) {
  await requireAiAdmin(db, actor);
  const input = { expectedVersion },
    operation = "ai.connection.test",
    prior = await replayCommand(db, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await connection(db, actor.companyId, id),
    rev = await revision(db, row);
  if (row.version !== expectedVersion)
    throw new ApiError(
      409,
      "AI_VERSION_CONFLICT",
      "The connection changed. Refresh before testing.",
    );
  let textVerified = false,
    imageVerified = false,
    error: string | null = null;
  try {
    const result = await generate(
      { ...await endpoint(env, actor.companyId, row, rev, "text"), sessionId: await sha256(`ai-test:${actor.companyId}:${id}:${rev.revision}`) },
      [
        {
          role: "system",
          content: "Return a JSON object with ok equal to true.",
        },
        {
          role: "user",
          content: 'Synthetic connection test. Return {"ok":true}.',
        },
      ],
    );
    textVerified = z
      .object({ ok: z.literal(true) })
      .safeParse(result.result).success;
    if (!textVerified)
      throw new ApiError(
        503,
        "AI_JSON_UNSUPPORTED",
        "The text model did not return the expected JSON test result.",
      );
    if (rev.image_model) {
      const vision = await generate(
        { ...await endpoint(env, actor.companyId, row, rev, "image"), sessionId: await sha256(`ai-test:${actor.companyId}:${id}:${rev.revision}`) },
        [
          {
            role: "system",
            content:
              "Identify the solid color of the supplied synthetic image. Return JSON with one string field named color. Use only the actual image. Use one lowercase English color name without descriptions or color codes.",
          },
          {
            role: "user",
            content: [
              { type: "text", text: "What color is this image?" },
              {
                type: "image_url",
                image_url: {
                  url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAQAAAAEACAIAAADTED8xAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAFG0lEQVR4nO3bwQ0AIQzEwPTftKmBVx4ZiQKQZXQH2Z1mLAS6qsH+DiwEcgBIgEC+ACRAIL9AJEAgdwASIJBLMAkQyCsQCRDIMygJEMgcgAQIZBBGAgQyCSYBAolCkACBZIFIgEDCcCRAIGlQEiCQODQJEEgfgAQIpBBDAgTSCCMBAqlEkgCBdIJJkEK2UjwJun0M9ndgIZADQAIE8gUgAQL5BSIBArkDkACBXIJJgEBegUiAQJ5BSYBA5gAkQCCDMBIgkEkwCRBIFIIECCQLRAIEEoYjAQJJg5IAgcShSYBA+gAkQCCFGBIgkEYYCRBIJZIECKQTTIIUspXiSdDtY7C/AwuBHAASIJAvAAkQyC8QCRDIHYAECOQSTAIE8gpEAgTyDEoCBDIHIAECGYSRAIFMgkmAQKIQJEAgWSASIJAwHAkQSBqUBAgkDk0CBNIHIAECKcSQAIE0wkiAQCqRJEAgnWASpJCtFE+Cbh+D/R1YCOQAkACBfAFIgEB+gUiAQO4AJEAgl2ASIJBXIBIgkGdQEiCQOQAJEMggjAQIZBJMAgQShSABAskCkQCBhOFIgEDSoCRAIHFoEiCQPgAJEEghhgQIaISRAIFRiSQBAqMTTAIElOJJMNePwf4OLARyAEiAQL4AJEAgv0AkQCB3ABIgkEswCRDIKxAJEMgzKAkQyByABAhkEEYCBDIJJgECiUKQAIFkgUiAQMJwJEAgaVASIJA4NAkQSB+ABAikEEMCBNIIIwECqUSSAIF0gkmQQrZSPAm6fQz2d2AhkANAAgTyBSABAvkFIgECuQOQAIFcgkmAQF6BSIBAnkFJgEDmACRAIIMwEiCQSTAJEEgUggQIJAtEAgQShiMBAkmDkgCBxKFJgED6ACRAIIUYEiCQRhgJEEglkgQIpBNMghSyleJJ0O1jsL8DC4EcABIgkC8ACRDILxAJEMgdgAQI5BJMAgTyCkQCBPIMSgIEMgcgAQIZhJEAgUyCSYBAohAkQCBZIBIgkDAcCRBIGpQECCQOTQIE0gcgAQIpxJAAgTTCSIBAKpEkQCCdYBKkkK0UT4JuH4P9HVgI5ACQAIF8AUiAQH6BSIBA7gAkQCCXYBIgkFcgEiCQZ1ASIJA5AAkQyCCMBAhkEkwCBBKFIAECyQKRAIGE4UiAQNKgJEAgcWgSIJA+AAkQSCGGBAikEUYCBFKJJAEC6QSTIIVspXgSdPsY7O/AQiAHgAQI5AtAAgTyC0QCBHIHIAECuQSTAIG8ApEAgTyDkgCBzAFIgEAGYSRAIJNgEiCQKAQJEEgWiAQIJAxHAgSSBiUBAolDkwCB9AFIgEAKMSRAII0wEiCQSiQJEEgnmAQpZCvFk6Dbx2B/BxYCOQAkQCBfABIgkF8gEiCQOwAJEMglmAQI5BWIBAjkGZQECGQOQAIEMggjAQKZBJMAgUQhSIBAskAkQCBhOBIgIA1KAgRGHJoECIw+AAkQGIUYEiAwGmEkQGBUIkmAwOgEkwABpXgSzPVjsL8DC4EcABIgkC8ACRDILxAJEMgdgAQI5BJMAgTyCkQCBPIMSgIEMgcgAQIZhJEAgUyCSYBAohAkQCBZIBIgkDAcCRBIGpQECCQOTQIE0gcgAQIpxJAAgTTCSIBAKpEkQCCdYBKkkK0UT4JuH4P9HVgI5ACQAIF8AUiAQH6BSIBA7gAkQCCXYBIgkFcgEiCQZ1ASIJA5AAkQyCCMBAhkEkwCBBKFIAECyQKRAIH+j8EDWUwO8hZP+cEAAAAASUVORK5CYII=",
                },
              },
            ],
          },
        ],
      );
      imageVerified = z
        .object({ color: z.string().trim().toLowerCase().pipe(z.literal("red")) })
        .safeParse(vision.result).success;
      if (!imageVerified)
        error = "The image model did not pass the synthetic image test.";
    }
  } catch (e) {
    error =
      e instanceof ApiError
        ? e.message
        : "The connection test failed. Check the endpoint and model.";
  }
  const now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    expectedVersion + 1,
    { id, version: expectedVersion + 1, textVerified, imageVerified, error },
    [
      guard(db, key, actor),
      versionGuard(db, key, row, expectedVersion),
      db
        .prepare(
          "UPDATE ai_connection_revisions SET tested_at=?,text_verified=?,image_verified=?,test_error=? WHERE connection_id=? AND revision=?",
        )
        .bind(
          now,
          textVerified ? 1 : 0,
          imageVerified ? 1 : 0,
          error,
          id,
          rev.revision,
        ),
      db
        .prepare(
          "UPDATE ai_connections SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
    ],
  );
}
export const aiDiscoverySchema = z.object({
  baseUrl: z.string().trim().max(500),
  authMode: z.enum(["bearer", "none"]).default("bearer"),
  apiKey: z.string().max(8000).optional(),
  connectionId: z.string().max(180).optional(),
  removeKey: z.boolean().default(false),
});
export async function discoverAiModelsInput(
  db: SqlDatabase, actor: StaffActor, env: BetaEnvironment,
  input: z.infer<typeof aiDiscoverySchema>,
) {
  await requireAiAdmin(db, actor);
  const baseUrl = aiBaseUrl(input.baseUrl);
  const row = input.connectionId ? await connection(db, actor.companyId, input.connectionId) : null;
  const rev = row ? await revision(db, row) : null;
  let apiKey = input.authMode === "bearer" ? input.apiKey?.trim() || null : null;
  if (!apiKey && input.authMode === "bearer" && !input.removeKey && rev?.credential_json) {
    if (baseUrl !== rev.base_url)
      throw new ApiError(422, "AI_KEY_REQUIRED", "Enter a new API key when changing the provider URL.");
    apiKey = await openAiKey(env, actor.companyId, row!.id, rev.credential_json);
  }
  // OpenCode publishes its catalog without authentication; inference still needs a key.
  if (!apiKey && input.authMode === "bearer" && presetForUrl(baseUrl) === "custom")
    throw new ApiError(422, "AI_KEY_REQUIRED", "Enter an API key to fetch models.");
  const raw = await providerRequest({ baseUrl, apiKey, model: "" }, "models");
  const parsed = z.object({ data: z.array(z.object({ id: z.string().trim().min(1).max(200) })) }).safeParse(raw);
  if (!parsed.success)
    throw new ApiError(503, "AI_INVALID_RESPONSE", "The provider returned an invalid model list.");
  const allModels = [...new Set(parsed.data.data.map(m => m.id))];
  return { models: supportedModels(baseUrl, allModels), totalModels: allModels.length, fetchedAt: new Date().toISOString(), message: null };
}
export async function discoverAiModels(db: SqlDatabase, actor: StaffActor, env: BetaEnvironment, id: string) {
  await requireAiAdmin(db, actor);
  const row = await connection(db, actor.companyId, id), rev = await revision(db, row);
  return discoverAiModelsInput(db, actor, env, { baseUrl: rev.base_url, authMode: rev.auth_mode, connectionId: id, removeKey: false });
}
export async function activateAiConnection(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  expectedVersion: number,
  key: string,
) {
  await requireAiAdmin(db, actor);
  const input = { expectedVersion },
    operation = "ai.connection.activate",
    prior = await replayCommand(db, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await connection(db, actor.companyId, id),
    rev = await revision(db, row);
  if (!rev.text_verified || (rev.image_model && !rev.image_verified))
    throw new ApiError(
      422,
      "AI_TEST_REQUIRED",
      "Test this revision successfully before activation.",
    );
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    expectedVersion + 1,
    { id, version: expectedVersion + 1 },
    [
      guard(db, key, actor),
      versionGuard(db, key, row, expectedVersion),
      assertion(
        db,
        key,
        "tested-revision",
        "EXISTS(SELECT 1 FROM ai_connection_revisions WHERE connection_id=? AND revision=? AND text_verified=1 AND (image_model IS NULL OR image_verified=1))",
        [id, row.draft_revision],
      ),
      db
        .prepare(
          "UPDATE ai_connections SET active_revision=draft_revision,disabled=0,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), id),
    ],
  );
}
export async function selectAiProviders(
  db: SqlDatabase,
  actor: StaffActor,
  input: z.infer<typeof aiSelectionSchema>,
  key: string,
) {
  await requireAiAdmin(db, actor);
  const operation = "ai.settings.save",
    prior = await replayCommand(
      db,
      actor,
      key,
      operation,
      actor.companyId,
      input,
    );
  if (prior) return prior;
  const checks: SqlStatement[] = [];
  for (const [id, kind] of (input.mode === "custom"
    ? [
        [input.textConnectionId, "text"],
        ...(input.imageMode === "custom"
          ? [[input.imageConnectionId, "image"]]
          : []),
      ]
    : []) as [string | null, "text" | "image"][]) {
    if (!id)
      throw new ApiError(
        422,
        "AI_CONNECTION_REQUIRED",
        "Choose an active connection.",
      );
    const row = await connection(db, actor.companyId, id),
      rev = await revision(db, row, true);
    if (
      row.disabled ||
      !rev.text_verified ||
      (kind === "image" && !rev.image_verified)
    )
      throw new ApiError(
        422,
        "AI_CAPABILITY_REQUIRED",
        "Choose an active connection with the required tested capability.",
      );
    checks.push(
      assertion(
        db,
        key,
        `selection-${kind}`,
        "EXISTS(SELECT 1 FROM ai_connections c JOIN ai_connection_revisions r ON r.connection_id=c.id AND r.revision=c.active_revision WHERE c.id=? AND c.company_id=? AND c.version=? AND c.disabled=0 AND c.deleted_at IS NULL AND r.text_verified=1 AND (?=0 OR r.image_verified=1))",
        [id, actor.companyId, row.version, kind === "image" ? 1 : 0],
      ),
    );
  }
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    actor.companyId,
    input.expectedVersion + 1,
    { version: input.expectedVersion + 1 },
    [
      guard(db, key, actor),
      assertion(
        db,
        key,
        "settings-version",
        "COALESCE((SELECT version FROM ai_company_settings WHERE company_id=?),0)=?",
        [actor.companyId, input.expectedVersion],
      ),
      ...checks,
      db
        .prepare(
          "INSERT INTO ai_company_settings(company_id,version,mode,text_connection_id,image_connection_id,image_mode,updated_by,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,mode=excluded.mode,text_connection_id=excluded.text_connection_id,image_connection_id=excluded.image_connection_id,image_mode=excluded.image_mode,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
        )
        .bind(
          actor.companyId,
          input.expectedVersion + 1,
          input.mode,
          input.mode === "custom" ? input.textConnectionId : null,
          input.mode === "custom" && input.imageMode === "custom"
            ? input.imageConnectionId
            : null,
          input.imageMode,
          actor.userId,
          Date.now(),
        ),
    ],
  );
}
export async function disableAiConnection(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  input: { expectedVersion: number; disableUses: boolean; remove: boolean },
  key: string,
) {
  await requireAiAdmin(db, actor);
  const operation = input.remove
      ? "ai.connection.delete"
      : "ai.connection.disable",
    prior = await replayCommand(db, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await connection(db, actor.companyId, id),
    settings = await db
      .prepare("SELECT * FROM ai_company_settings WHERE company_id=?")
      .bind(actor.companyId)
      .first<Selection>();
  const used =
    settings?.mode === "custom" &&
    (settings.text_connection_id === id ||
      (settings.image_mode === "custom" &&
        settings.image_connection_id === id));
  if (used && !input.disableUses)
    throw new ApiError(
      409,
      "AI_CONNECTION_IN_USE",
      "Choose a replacement first, or explicitly disable AI using this connection.",
    );
  const now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      guard(db, key, actor),
      versionGuard(db, key, row, input.expectedVersion),
      assertion(
        db,
        key,
        "settings-version",
        "COALESCE((SELECT version FROM ai_company_settings WHERE company_id=?),0)=?",
        [actor.companyId, settings?.version ?? 0],
      ),
      ...(used
        ? [
            db
              .prepare(
                "UPDATE ai_company_settings SET mode=CASE WHEN text_connection_id=? THEN 'disabled' ELSE mode END,image_mode=CASE WHEN image_connection_id=? THEN 'disabled' ELSE image_mode END,version=version+1,updated_by=?,updated_at=? WHERE company_id=?",
              )
              .bind(id, id, actor.userId, now, actor.companyId),
          ]
        : []),
      db
        .prepare(
          "UPDATE ai_connections SET disabled=1,deleted_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(input.remove ? now : null, now, id),
      ...(input.remove
        ? [
            db
              .prepare(
                "UPDATE ai_connection_revisions SET credential_json=NULL WHERE connection_id=?",
              )
              .bind(id),
          ]
        : []),
    ],
  );
}
export async function resolveAiProvider(
  db: SqlDatabase,
  companyId: string,
  env: BetaEnvironment,
  kind: "text" | "image",
) {
  const settings = await db
    .prepare("SELECT * FROM ai_company_settings WHERE company_id=?")
    .bind(companyId)
    .first<Selection>();
  if (
    settings?.mode === "disabled" ||
    (kind === "image" && settings?.image_mode === "disabled")
  )
    throw new ApiError(
      503,
      "AI_DISABLED",
      "AI is disabled for this capability. Continue manually.",
    );
  if (!settings || settings.mode === "deployment") {
    if (!env.AI_BASE_URL || !env.AI_MODEL || !env.AI_API_KEY)
      throw new ApiError(
        503,
        "AI_UNAVAILABLE",
        "No AI connection is configured. Continue manually or ask an owner or IT administrator to configure AI providers.",
      );
    if (kind === "image")
      throw new ApiError(
        503,
        "AI_IMAGE_TEST_REQUIRED",
        "Configure and test an image model in AI providers to read documents.",
      );
    const { sha256 } = await import("./crypto");
    return {
      endpoint: {
        baseUrl: env.AI_BASE_URL,
        model: env.AI_MODEL,
        apiKey: env.AI_API_KEY,
      },
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
    kind === "image" && settings.image_mode === "custom"
      ? settings.image_connection_id
      : settings.text_connection_id;
  if (!id)
    throw new ApiError(
      503,
      "AI_UNAVAILABLE",
      "Choose an AI connection in Administration.",
    );
  const row = await connection(db, companyId, id),
    rev = await revision(db, row, true);
  if (
    row.disabled ||
    !rev.text_verified ||
    (kind === "image" && !rev.image_verified)
  )
    throw new ApiError(
      503,
      "AI_CAPABILITY_UNAVAILABLE",
      "The selected connection is disabled or has not passed the required capability test.",
    );
  const target = await endpoint(env, companyId, row, rev, kind);
  return {
    endpoint: target,
    snapshot: {
      companyId,
      connectionId: id,
      revision: rev.revision,
      settingsVersion: settings.version,
      model: target.model,
      kind,
    } satisfies AiProviderSnapshot,
  };
}
export function aiSnapshotGuard(
  db: SqlDatabase,
  key: string,
  snapshot: AiProviderSnapshot,
  label = "ai-provider-revision",
) {
  const settings =
    "COALESCE((SELECT version FROM ai_company_settings WHERE company_id=?),0)=?";
  return assertion(
    db,
    key,
    label,
    `${settings}${snapshot.connectionId ? " AND EXISTS(SELECT 1 FROM ai_connections c JOIN ai_connection_revisions r ON r.connection_id=c.id AND r.revision=c.active_revision WHERE c.id=? AND c.company_id=? AND c.active_revision=? AND c.disabled=0 AND c.deleted_at IS NULL AND r.text_verified=1 AND (?='text' OR r.image_verified=1))" : " AND COALESCE((SELECT mode FROM ai_company_settings WHERE company_id=?),'deployment')='deployment'"}`,
    [
      snapshot.companyId,
      snapshot.settingsVersion,
      ...(snapshot.connectionId
        ? [
            snapshot.connectionId,
            snapshot.companyId,
            snapshot.revision,
            snapshot.kind,
          ]
        : [snapshot.companyId]),
    ],
  );
}
export function aiUsageStatement(
  db: SqlDatabase,
  id: string,
  snapshot: AiProviderSnapshot,
  feature: string,
  status: "succeeded" | "failed",
  inputTokens = 0,
  outputTokens = 0,
) {
  return db
    .prepare(
      "INSERT INTO ai_provider_usage(id,company_id,connection_id,revision,model,feature,status,input_tokens,output_tokens,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,input_tokens=excluded.input_tokens,output_tokens=excluded.output_tokens",
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
    );
}
