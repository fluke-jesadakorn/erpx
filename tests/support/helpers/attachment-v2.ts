import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { createHash } from "node:crypto";
import { ApiError } from "@erpx/server-runtime/public/http";

type SeedAttachmentSetInput = {
  targetType: string;
  targetId: string;
  componentKind?: "record" | "action" | "line" | "event" | "message" | "publication";
  componentRef?: string;
  purposeKey: string;
  expectedTargetVersion: number;
  sourceState?: "legacy_unscanned" | "quarantined" | "scanning" | "available" | "scan_error" | "malware_rejected" | "destroyed";
  mediaType?: string;
  requiredCount?: number;
  compatibilityAdapter?: boolean;
};

/**
 * Seeds the post-scan side of attachment intake so domain tests can exercise
 * the real bind guards and transaction without depending on clamd or preview
 * workers. This helper intentionally does not call a domain command.
 */
export async function seedAttachmentSet(
  db: SqlDatabase,
  actor: WorkContext,
  input: SeedAttachmentSetInput,
) {
  const token = crypto.randomUUID();
  const setId = `test-attachment-set-${token}`;
  const sourceId = `test-attachment-source-${token}`;
  const linkId = `test-attachment-link-${token}`;
  const now = Date.now();
  const mediaType = input.mediaType ?? "application/pdf";
  const sourceState = input.sourceState ?? "available";
  const filename = mediaType.startsWith("image/") ? "evidence.png" : "evidence.pdf";
  const checksum = token.replaceAll("-", "").padEnd(64, "0").slice(0, 64);
  const uploadId = `test-upload-${token}`;

  await db.batch([
    db.prepare(`INSERT INTO upload_intents(
      id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,
      staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type
    ) VALUES(?,?,?,?,?,?,8,8,?,?,?,'ready',?,?,?,?,?,?,?)`).bind(
      uploadId,
      input.targetType,
      input.targetId,
      input.purposeKey,
      filename,
      mediaType,
      checksum,
      `test/staging/${token}`,
      `test/attachments/${token}`,
      actor.userId,
      now + 15 * 60_000,
      now,
      now,
      input.expectedTargetVersion,
      actor.companyId,
      mediaType,
    ),
    db.prepare(`INSERT INTO attachments(
      id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,
      checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at
    ) VALUES(?,?,?,?,?,?,1,?,?,8,?,?,?,?,?,?,?,?)`).bind(
      sourceId,
      uploadId,
      input.targetType,
      input.targetId,
      input.purposeKey,
      `test/attachments/${token}`,
      filename,
      mediaType,
      checksum,
      actor.userId,
      now,
      actor.companyId,
      mediaType,
      sourceState,
      sourceState === "available" ? now : null,
      sourceState === "available" ? now : null,
    ),
    db.prepare(`INSERT INTO attachment_sets(
      id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,
      expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,
      created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,?,?,?,?, ?,1,'internal','{}',?,10,'draft',?,?,?)`).bind(
      setId,
      actor.companyId,
      actor.branchId,
      actor.departmentId,
      input.targetType,
      input.targetId,
      input.componentKind ?? "action",
      input.componentRef ?? null,
      input.purposeKey,
      input.expectedTargetVersion,
      input.requiredCount ?? 0,
      actor.userId,
      now,
      now,
    ),
    db.prepare(`INSERT INTO attachment_links(
      id,set_id,attachment_id,state,created_by,created_at,expires_at
    ) VALUES(?,?,?,'draft',?,?,?)`).bind(
      linkId,
      setId,
      sourceId,
      actor.userId,
      now,
      now + 7 * 24 * 60 * 60_000,
    ),
    ...(input.compatibilityAdapter
      ? [db.prepare(`INSERT INTO attachment_events(
          id,company_id,set_id,actor_id,action,details_json,created_at
        ) VALUES(?,?,?,?, 'set.created',?,?)`).bind(
          `test-attachment-event-${token}`,
          actor.companyId,
          setId,
          actor.userId,
          JSON.stringify({ compatibilityAdapter: true }),
          now,
        )]
      : []),
  ]);

  return { setId, sourceId, linkId };
}

export async function attachmentBindingState(db: SqlDatabase, setId: string) {
  return db.prepare(`SELECT s.state AS set_state,s.bound_command_id,l.state AS link_state,l.binding_type,l.binding_id
    FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=?`).bind(setId).first<{
      set_state: string;
      bound_command_id: string | null;
      link_state: string;
      binding_type: string | null;
      binding_id: string | null;
    }>();
}

export function enableAttachmentV2ForTest() {
  process.env.ERPX_ATTACHMENTS_V2 = "true";
}

type WithdrawSeedOptions = {
  /** Owner-record authority the native composition port would enforce. */
  authorize?: (
    database: SqlDatabase,
    facts: {
      targetType: string;
      targetId: string;
      purposeKey: string;
      componentKind: string;
      createdBy: string;
    },
  ) => Promise<void>;
  /** Map a persistence failure raised by a deferred owner assertion. */
  failure?: (error: unknown) => unknown;
  /** Refuse an already-open arbitrary transaction for an owner that requires
   * its own Files unit, preserving the native command's transaction boundary. */
  requireOwnedUnit?: string;
  /** Re-check the owner source after deferred effects. The former TypeScript
   * writer flushed `SET CONSTRAINTS ALL IMMEDIATE`; a native owner command
   * performs the equivalent source check in its own authority port. */
  after?: (database: SqlDatabase) => Promise<void>;
};

/**
 * Native Files withdrawal oracle. This performs the same staged-link state
 * change and audit event as the Rust owner command so domain tests can retire
 * the former TypeScript writer without inventing a second production path.
 * Owner-record authority is supplied by the caller through `authorize`.
 */
export async function withdrawSeededAttachment(
  db: SqlDatabase,
  actor: WorkContext,
  linkId: string,
  reason: string,
  idempotencyKey: string,
  options: WithdrawSeedOptions = {},
): Promise<{ id: string; state: "withdrawn" }> {
  const text = reason.trim();
  if (text.length < 3 || text.length > 1000)
    throw new ApiError(422, "WITHDRAWAL_REASON_REQUIRED", "Explain why this file is being withdrawn.");
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(idempotencyKey))
    throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required.");
  if (options.requireOwnedUnit && !isTransactionalSqlDatabase(db))
    throw new ApiError(409, options.requireOwnedUnit, "This file command requires its own owner transaction.");
  const run = async (database: SqlDatabase): Promise<{ id: string; state: "withdrawn" }> => {
    try {
      const row = await database
        .prepare(
          `SELECT l.state,l.created_by,l.withdrawn_by,l.withdrawal_reason,s.state set_state,
             s.target_type,s.target_id,s.purpose_key,s.component_kind,s.company_id
           FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
           WHERE l.id=? AND s.company_id=?`,
        )
        .bind(linkId, actor.companyId)
        .first<{
          state: string;
          created_by: string;
          withdrawn_by: string | null;
          withdrawal_reason: string | null;
          set_state: string;
          target_type: string;
          target_id: string;
          purpose_key: string;
          component_kind: string;
          company_id: string;
        }>();
      if (!row) throw new ApiError(404, "ATTACHMENT_LINK_NOT_FOUND", "File link not found.");
      if (row.state === "bound")
        throw new ApiError(409, "BOUND_ATTACHMENT_IMMUTABLE", "Bound evidence cannot be withdrawn. Add a correction instead.");
      if (row.created_by !== actor.userId)
        throw new ApiError(403, "ATTACHMENT_WITHDRAW_FORBIDDEN", "Only the person who staged this file may withdraw it.");
      await options.authorize?.(database, {
        targetType: row.target_type,
        targetId: row.target_id,
        purposeKey: row.purpose_key,
        componentKind: row.component_kind,
        createdBy: row.created_by,
      });
      if (row.target_type === "leave_request") {
        const leave = await database
          .prepare("SELECT state,created_by,version FROM leave_requests WHERE id=? AND company_id=?")
          .bind(row.target_id, actor.companyId)
          .first<{ state: string; created_by: string; version: number }>();
        const editable =
          leave !== null &&
          leave.created_by === actor.userId &&
          ["draft", "rejected", "pending_approval"].includes(leave.state);
        if (!editable)
          throw new ApiError(
            403,
            "LEAVE_EVIDENCE_REQUESTER_REQUIRED",
            "Only the requester can add or withdraw leave evidence before final approval.",
          );
      }
      if (row.state === "withdrawn") {
        if (row.withdrawn_by === actor.userId && row.withdrawal_reason === text)
          return { id: linkId, state: "withdrawn" as const };
        throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This file was withdrawn with a different command.");
      }
      if (row.state !== "draft")
        throw new ApiError(409, "ATTACHMENT_LINK_LOCKED", "This file link is closed.");
      if (row.set_state !== "draft")
        throw new ApiError(409, "ATTACHMENT_SET_LOCKED", "This file set is already bound or closed.");
      const now = Date.now();
      const updated = await database
        .prepare(
          "UPDATE attachment_links l SET state='withdrawn',withdrawn_at=?,withdrawn_by=?,withdrawal_reason=? FROM attachment_sets s WHERE l.id=? AND l.set_id=s.id AND s.company_id=? AND l.state='draft'",
        )
        .bind(now, actor.userId, text, linkId, actor.companyId)
        .run();
      if (updated.meta.changes !== 1)
        throw new ApiError(409, "ATTACHMENT_LINK_LOCKED", "This file link is closed.");
      const eventId = `attachment-withdraw-${createHash("sha256").update(`${actor.companyId}\0${linkId}\0${idempotencyKey}`).digest("hex").slice(0, 32)}`;
      await database
        .prepare(
          "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) SELECT ?,s.company_id,l.attachment_id,l.id,l.set_id,?,'link.withdrawn',?,? FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=? ON CONFLICT(id) DO NOTHING",
        )
        .bind(eventId, actor.userId, JSON.stringify({ reason: text }), now, linkId, actor.companyId)
        .run();
      const intact = await database
        .prepare(
          "SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=? AND company_id=? AND link_id=? AND actor_id=? AND action='link.withdrawn' AND details_json=? AND created_at=?)",
        )
        .bind(eventId, actor.companyId, linkId, actor.userId, JSON.stringify({ reason: text }), now)
        .first<boolean>();
      if (!intact)
        throw new ApiError(409, "ATTACHMENT_LINK_LOCKED", "This file link is closed.");
      if (options.after) {
        await database.prepare("SET CONSTRAINTS ALL IMMEDIATE").run();
        await options.after(database);
      }
      return { id: linkId, state: "withdrawn" as const };
    } catch (error) {
      const mapped = options.failure?.(error);
      throw mapped ?? error;
    }
  };
  try {
    return isTransactionalSqlDatabase(db) ? await db.transaction(run) : await run(db);
  } catch (error) {
    const mapped = options.failure?.(error);
    throw mapped ?? error;
  }
}
