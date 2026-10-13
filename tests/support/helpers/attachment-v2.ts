import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";

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
