import { supportedModelProtocol } from "@erpx/feature-shared/lib/ai/providers";
import { sealAiKey } from "@erpx/server-agent/public/ai-transport";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

type AiEnv = {
  AI_CREDENTIAL_KEY_ID?: string;
  AI_CREDENTIAL_KEYS?: string;
};
type Actor = { companyId: string; userId: string };

export type AiProviderSeed = {
  name?: string;
  baseUrl: string;
  apiKey?: string;
  textModel: string;
  imageModel?: string;
  providerPreset?: "custom" | "opencode-zen" | "opencode-go";
};

/**
 * Owner-row seeding for tests that need a verified, active company provider.
 * This replaces the retired Administration TS writer; it never exercises the
 * product command path and keeps the sealed credential shape unchanged.
 */
export async function seedAiProvider(
  db: SqlDatabase,
  env: AiEnv,
  actor: Actor,
  input: AiProviderSeed,
): Promise<{ id: string; version: number }> {
  const id = `ai-${crypto.randomUUID()}`,
    now = Date.now(),
    imageModel = input.imageModel?.trim() || null,
    credential = input.apiKey
      ? await sealAiKey(env, actor.companyId, id, input.apiKey)
      : null,
    textProtocol =
      supportedModelProtocol(input.baseUrl, input.textModel) ?? "chat-completions",
    imageProtocol =
      (imageModel && supportedModelProtocol(input.baseUrl, imageModel)) ??
      "chat-completions";
  await db.batch([
    db
      .prepare(
        "INSERT INTO ai_connections(id,company_id,name,version,draft_revision,active_revision,disabled,deleted_at,created_by,created_at,updated_at) VALUES(?,?,?,3,1,1,0,NULL,?,?,?)",
      )
      .bind(id, actor.companyId, input.name ?? "Seeded provider", actor.userId, now, now),
    db
      .prepare(
        "INSERT INTO ai_connection_revisions(connection_id,revision,base_url,auth_mode,credential_json,text_model,image_model,tested_at,text_verified,image_verified,test_error,created_by,created_at,provider_preset,text_protocol,image_protocol) VALUES(?,1,?,?,?,?,?,?,1,?,NULL,?,?,?,?,?)",
      )
      .bind(
        id,
        input.baseUrl,
        credential ? "bearer" : "none",
        credential,
        input.textModel,
        imageModel,
        now,
        imageModel ? 1 : 0,
        actor.userId,
        now,
        input.providerPreset ?? "custom",
        textProtocol,
        imageProtocol,
      ),
    db
      .prepare(
        "INSERT INTO ai_company_settings(company_id,version,mode,text_connection_id,image_connection_id,image_mode,updated_by,updated_at,home_ai_name) VALUES(?,1,'custom',?,NULL,'default',?,?,'AI') ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,mode=excluded.mode,text_connection_id=excluded.text_connection_id,image_connection_id=excluded.image_connection_id,image_mode=excluded.image_mode,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
      )
      .bind(actor.companyId, id, actor.userId, now),
  ]);
  return { id, version: 3 };
}

/** Seed a company selection without exercising the retired select writer. */
export async function seedAiSelection(
  db: SqlDatabase,
  actor: Actor,
  input: {
    mode: "deployment" | "custom" | "disabled";
    version?: number;
    textConnectionId?: string | null;
    imageMode?: "default" | "custom" | "disabled";
    imageConnectionId?: string | null;
  },
) {
  const now = Date.now();
  await db
    .prepare(
      "INSERT INTO ai_company_settings(company_id,version,mode,text_connection_id,image_connection_id,image_mode,updated_by,updated_at,home_ai_name) VALUES(?,?,?,?,?,?,?,?,'AI') ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,mode=excluded.mode,text_connection_id=excluded.text_connection_id,image_connection_id=excluded.image_connection_id,image_mode=excluded.image_mode,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
    )
    .bind(
      actor.companyId,
      input.version ?? 1,
      input.mode,
      input.textConnectionId ?? null,
      input.imageConnectionId ?? null,
      input.imageMode ?? "default",
      actor.userId,
      now,
    )
    .run();
}
