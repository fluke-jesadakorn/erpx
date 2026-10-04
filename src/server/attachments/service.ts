import "server-only";

import { createHash, randomUUID } from "node:crypto";
import type { WorkContext } from "../authorization";
import { publicScope } from "../authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { assertion, executeCommand, replayCommand } from "../commands";
import { sha256 } from "../crypto";
import { ApiError } from "../http";
import { salesRecipientEligibility } from "../sales-recipient";
import type { FileStore, SqlDatabase, SqlStatement } from "../runtime/types";
import { isTransactionalSqlDatabase } from "../runtime/types";
import { ATTACHMENT_DRAFT_MS, ATTACHMENT_RESERVATION_MS, attachmentUploadsFrozen, attachmentsV2Enabled, attachmentsV2ReadOnly, requireAttachmentsV2, requireAttachmentsV2Read } from "./feature";
import { assertAttachmentMedia, attachmentPolicy, attachmentPolicyRegistry, attachmentRetentionSnapshot, authorizeAttachmentTarget } from "./policy";
import { attachmentPreviewReadiness, detectedAttachmentMediaType, sanitizedAttachmentPreview } from "./preview";
import { portalCollaborationAttachmentRecordVersionStep,portalCollaborationAttachmentTargetVersionGuard } from "./portal-collaboration-policy";
import { attachmentScannerReadiness, scanAttachmentBytes } from "./scanner";
import { sensitiveAttachmentRecordVersionStep,sensitiveAttachmentTargetVersionGuard } from "./sensitive-policy";
import { assertSanitizedEvidenceMetadata,financeOperationsAttachmentRecordVersionStep,financeOperationsAttachmentTargetVersionGuard,requiresSanitizedEvidenceAttestation } from "./finance-operations-policy";
import { remainingOperationalAttachmentRecordVersionStep,remainingOperationalAttachmentTargetVersionGuard } from "./remaining-operational-policy";
import type {
  AttachmentLinkDto,
  AttachmentManifestItem,
  AttachmentSetDto,
  AttachmentBindingContract,
  AttachmentPolicy,
  AttachmentTargetScope,
  PreparedAttachmentBinding,
} from "./types";
import type { AttachmentComponentKind, AttachmentLinkState, AttachmentSourceState } from "./types";

type SetRow = {
  id: string; company_id: string; branch_id: string | null; department_id: string | null;
  target_type: string; target_id: string; component_kind: AttachmentComponentKind; component_ref: string | null;
  purpose_key: string; expected_target_version: number; policy_version: number; classification: AttachmentSetDto["classification"];
  audience_json: string;
  required_count: number; max_files: number; state: AttachmentSetDto["state"]; created_by: string; created_at: number; updated_at: number;
};
type LinkRow = {
  id: string; set_id: string; attachment_id: string | null; upload_intent_id: string | null; caption: string | null;
  state: AttachmentLinkState; created_by: string; created_at: number; expires_at: number; bound_at: number | null;
  original_name: string | null; media_type: string | null; declared_bytes: number | null; bytes: number | null;
  source_state: AttachmentSourceState | null; preview_object_key: string | null;
};

const brandPublicationTargets = new Set(["holding_brand","branch_brand","company_storefront_brand"]);
function isBrandPublicationTarget(targetType:string) { return brandPublicationTargets.has(targetType); }

async function atomic<T>(db: SqlDatabase, action: (tx: SqlDatabase) => Promise<T>) {
  return isTransactionalSqlDatabase(db) ? db.transaction(action) : action(db);
}

function stableId(prefix: string, actor: WorkContext, key: string) {
  return `${prefix}-${createHash("sha256").update(`${actor.companyId}\0${actor.userId}\0${key}`).digest("hex").slice(0, 32)}`;
}

function safeJson(value: unknown) { return JSON.stringify(value).replaceAll("<", "\\u003c"); }

function safeUploadErrorCode(error: unknown) {
  const code = error && typeof error === "object" && "code" in error ? error.code : undefined;
  return typeof code === "string" && /^[A-Za-z0-9_]{1,80}$/.test(code) ? code : undefined;
}

type AttachmentServiceOptions={compatibilityAdapter?:boolean;requestId?:string};
function logUploadOperationFailure(options:AttachmentServiceOptions,operation:string,error:unknown){
  if(!options.requestId || error instanceof ApiError && error.status<500)return;
  const errorName=error instanceof Error&&/^[A-Za-z0-9_]{1,80}$/.test(error.name)?error.name:typeof error;
  console.error(JSON.stringify({event:"attachment_upload_operation_failed",requestId:options.requestId,operation,errorName,errorCode:safeUploadErrorCode(error)}));
}
function attachmentServiceEnvironment(options:AttachmentServiceOptions={}){
  return options.compatibilityAdapter&&!attachmentsV2Enabled()?{...process.env,ERPX_ATTACHMENTS_V2:"true"}:process.env;
}
function requireAttachmentWrite(options:AttachmentServiceOptions={}){
  if(!options.compatibilityAdapter)return requireAttachmentsV2();
  if(attachmentsV2ReadOnly())throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
}
function requireAttachmentRead(options:AttachmentServiceOptions={}){
  if(!options.compatibilityAdapter)return requireAttachmentsV2Read();
}

async function requireScannerReady(environment:NodeJS.ProcessEnv=process.env) {
  const [readiness,preview] = await Promise.all([attachmentScannerReadiness(environment),attachmentPreviewReadiness(environment)]);
  if (!readiness.ready || !readiness.enabled || !preview.ready)
    throw new ApiError(503, "ATTACHMENT_SCANNER_UNAVAILABLE", "File intake is unavailable until the malware scanner and current signatures are ready.");
  return readiness;
}

async function setRow(db: SqlDatabase, id: string, companyId: string) {
  const row = await db.prepare("SELECT * FROM attachment_sets WHERE id=? AND company_id=?").bind(id,companyId).first<SetRow>();
  if (!row) throw new ApiError(404, "ATTACHMENT_SET_NOT_FOUND", "Attachment set not found.");
  return row;
}

/**
 * Leave approval and evidence intake share the leave_requests row as their
 * serialization boundary. Callers must invoke this inside the same database
 * transaction that mutates the attachment set/link/source. The second
 * authorization check is intentional: a requester can lose write access, or
 * the request can advance to a new version/final state, while bytes are being
 * selected and uploaded.
 */
async function lockLeaveAttachmentTargetRow(
  db: SqlDatabase,
  actor: WorkContext,
  targetType: string,
  targetId: string,
) {
  if (targetType !== "leave_request") return false;
  if(actor.authority){
    // Match executeCommand's lock order (active context -> assignment -> Leave)
    // so a revocation that wins the race is observed, while one that follows
    // waits until this already-authorized mutation commits.
    const active=await db.prepare(`SELECT assignment_id,assignment_version,revision,expires_at,user_id
      FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE`)
      .bind(actor.authority.sessionId,actor.companyId).first<{assignment_id:string|null;assignment_version:number|null;revision:number;expires_at:number;user_id:string}>();
    if(!active||active.user_id!==actor.userId||active.assignment_id!==actor.assignmentId||Number(active.assignment_version)!==actor.authority.assignmentVersion||Number(active.revision)!==actor.authority.revision||Number(active.expires_at)<=Date.now())
      throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload before changing Leave evidence.");
    const assignment=await db.prepare(`SELECT id FROM assignments a WHERE a.id=? AND a.version=? AND a.user_id=? AND a.role=? AND a.company_id=?
      AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?))
      AND EXISTS(SELECT 1 FROM organization_nodes company WHERE company.id=a.company_id AND company.status='active')
      AND (a.scope_kind='holding' OR EXISTS(SELECT 1 FROM organization_nodes node WHERE node.id=a.scope_node_id AND node.company_id=a.company_id AND node.status='active'
        AND (node.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes branch WHERE branch.id=node.branch_id AND branch.company_id=a.company_id AND branch.status='active'))))
      FOR SHARE`).bind(actor.assignmentId,actor.authority.assignmentVersion,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{id:string}>();
    if(!assignment)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your assignment changed. Reload before changing Leave evidence.");
  }
  const locked = await db
    .prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE")
    .bind(targetId, actor.companyId)
    .first<{ id: string }>();
  if (!locked)
    throw new ApiError(404, "NOT_FOUND", "Leave request not found.");
  return true;
}

async function reauthorizeLockedLeaveAttachment(
  db: SqlDatabase,
  actor: WorkContext,
  input: {
    targetType: string;
    targetId: string;
    purposeKey: string;
    componentKind: AttachmentComponentKind;
    expectedTargetVersion: number;
  },
  resolvedPolicy?: AttachmentPolicy,
  operation: "write" | "read" = "write",
): Promise<AttachmentTargetScope | null> {
  if (input.targetType !== "leave_request") return null;
  const scope = await authorizeAttachmentTarget(
    db,
    actor,
    resolvedPolicy ?? attachmentPolicy(input.targetType, input.purposeKey, input.componentKind),
    input.targetId,
    operation,
  );
  if (scope.companyId !== actor.companyId)
    throw new ApiError(403, "ATTACHMENT_CROSS_COMPANY", "Files cannot cross Companies.");
  if (operation === "write" && scope.version !== Number(input.expectedTargetVersion))
    throw new ApiError(
      409,
      "ATTACHMENT_TARGET_CHANGED",
      "The leave request changed. Refresh before changing its evidence.",
    );
  return scope;
}

async function lockLeaveAttachmentMutation(
  db: SqlDatabase,
  actor: WorkContext,
  input: Parameters<typeof reauthorizeLockedLeaveAttachment>[2],
  resolvedPolicy?: AttachmentPolicy,
) {
  if (!await lockLeaveAttachmentTargetRow(db, actor, input.targetType, input.targetId)) return null;
  return reauthorizeLockedLeaveAttachment(db, actor, input, resolvedPolicy, "write");
}

async function setLinks(db: SqlDatabase, id: string, companyId: string) {
  return (await db.prepare(`SELECT l.*,a.original_name,a.media_type,a.bytes,a.source_state,a.preview_object_key,
      u.original_name AS intent_original_name,u.media_type AS intent_media_type,u.declared_bytes
    FROM attachment_links l
    JOIN attachment_sets s ON s.id=l.set_id
    LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
    WHERE l.set_id=? AND s.company_id=?
      AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
      AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)
    ORDER BY l.created_at,l.id`).bind(id,companyId).all<LinkRow & {intent_original_name:string|null;intent_media_type:string|null}>()).results
    .map((row) => ({...row,original_name:row.original_name??row.intent_original_name,media_type:row.media_type??row.intent_media_type}));
}

function linkDto(row: LinkRow): AttachmentLinkDto {
  return {
    id: row.id, sourceId: row.attachment_id, filename: row.original_name ?? "file", mediaType: row.media_type ?? "application/octet-stream",
    bytes: row.bytes ?? row.declared_bytes, sourceState: row.source_state, linkState: row.state, caption: row.caption,
    previewAvailable: !!row.preview_object_key, createdAt: Number(row.created_at), expiresAt: Number(row.expires_at), boundAt: row.bound_at === null ? null : Number(row.bound_at),
  };
}

async function setDto(db: SqlDatabase, row: SetRow): Promise<AttachmentSetDto> {
  const sanitizationAttested=requiresSanitizedEvidenceAttestation(row.target_type)
    ? Boolean(await db.prepare(`SELECT 1 FROM attachment_events attested WHERE attested.set_id=? AND attested.company_id=? AND attested.action='sanitization.attested'
        AND attested.created_at>=COALESCE((SELECT max(changed.created_at) FROM attachment_events changed WHERE changed.set_id=attested.set_id AND changed.action IN ('file.reserved','link.reused','link.withdrawn')),0)
        ORDER BY attested.created_at DESC LIMIT 1`).bind(row.id,row.company_id).first())
    : undefined;
  return {
    id: row.id, targetType: row.target_type, targetId: row.target_id, componentKind: row.component_kind,
    componentRef: row.component_ref, purposeKey: row.purpose_key, expectedTargetVersion: Number(row.expected_target_version),
    policyVersion: Number(row.policy_version), classification: row.classification, requiredCount: Number(row.required_count),
    maxFiles: Number(row.max_files), state: row.state, createdAt: Number(row.created_at), updatedAt: Number(row.updated_at), sanitizationAttested,
    links: (await setLinks(db,row.id,row.company_id)).map(linkDto),
  };
}

export async function listAttachmentSets(db: SqlDatabase, actor: WorkContext, input: {targetType:string;targetId:string;purposeKey?:string;componentKind?:AttachmentComponentKind;expectedTargetVersion?:number}) {
  requireAttachmentsV2Read();
  if (!input.targetType || !input.targetId) throw new ApiError(422,"ATTACHMENT_TARGET_REQUIRED","Choose a record before listing files.");
  const readOnly=attachmentsV2ReadOnly(),leaveReview=input.targetType==="leave_request";
  const filters = ["company_id=?","target_type=?","target_id=?",readOnly?"state='bound'":leaveReview?"state IN ('draft','bound')":"(state='bound' OR (state='draft' AND created_by=?))"], values: unknown[] = [actor.companyId,input.targetType,input.targetId];
  if(!readOnly&&!leaveReview)values.push(actor.userId);
  if (input.purposeKey) { filters.push("purpose_key=?"); values.push(input.purposeKey); }
  if (input.componentKind) { filters.push("component_kind=?"); values.push(input.componentKind); }
  if (input.expectedTargetVersion) { filters.push("expected_target_version=?"); values.push(input.expectedTargetVersion); }
  const rows = (await db.prepare(`SELECT * FROM attachment_sets WHERE ${filters.join(" AND ")} ORDER BY created_at DESC,id DESC`).bind(...values).all<SetRow>()).results;
  for (const row of rows) await authorizeAttachmentTarget(db,actor,attachmentPolicy(row.target_type,row.purpose_key,row.component_kind),row.target_id,"read");
  return Promise.all(rows.map((row) => setDto(db,row)));
}

export async function createAttachmentSet(db: SqlDatabase, actor: WorkContext, input: {targetType:string;targetId:string;componentKind:AttachmentComponentKind;componentRef?:string;purposeKey:string;expectedTargetVersion:number}, idempotencyKey: string, options:AttachmentServiceOptions={}) {
  requireAttachmentWrite(options);
  if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  await requireScannerReady(attachmentServiceEnvironment(options));
  const policy = attachmentPolicy(input.targetType,input.purposeKey,input.componentKind);
  const retention=attachmentRetentionSnapshot(policy);
  const scope = await authorizeAttachmentTarget(db,actor,policy,input.targetId,"write");
  if (scope.companyId !== actor.companyId) throw new ApiError(403,"ATTACHMENT_CROSS_COMPANY","Files cannot cross Companies.");
  if (scope.version !== input.expectedTargetVersion) throw new ApiError(409,"ATTACHMENT_TARGET_CHANGED","The record changed. Refresh before adding files.");
  const componentRef = input.componentRef?.trim() || null;
  if (componentRef && componentRef.length > 180) throw new ApiError(422,"ATTACHMENT_COMPONENT_REF","The file context is too long.");
  const now=Date.now(), id=stableId("attachment-set",actor,idempotencyKey);
  return atomic(db,async(tx)=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-set:${actor.companyId}:${actor.userId}:${idempotencyKey}`).run();
    const lockedScope=await lockLeaveAttachmentMutation(tx,actor,input,policy)??scope;
    const prior=await tx.prepare("SELECT * FROM attachment_sets WHERE id=? AND company_id=?").bind(id,actor.companyId).first<SetRow>();
    if(prior){
      const priorCompatibility=Boolean(await tx.prepare("SELECT 1 FROM attachment_events WHERE set_id=? AND company_id=? AND action='set.created' AND COALESCE((details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true LIMIT 1").bind(id,actor.companyId).first());
      if(prior.target_type!==input.targetType||prior.target_id!==input.targetId||prior.component_kind!==input.componentKind||prior.component_ref!==componentRef||prior.purpose_key!==input.purposeKey||Number(prior.expected_target_version)!==input.expectedTargetVersion||priorCompatibility!==Boolean(options.compatibilityAdapter))
        throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another attachment set.");
      return setDto(tx,prior);
    }
    const reusable=await tx.prepare(`SELECT candidate.* FROM attachment_sets candidate WHERE candidate.company_id=? AND candidate.target_type=? AND candidate.target_id=? AND candidate.component_kind=?
      AND COALESCE(candidate.component_ref,'')=COALESCE(?,'') AND candidate.purpose_key=? AND candidate.expected_target_version=? AND candidate.created_by=? AND candidate.state='draft'
      AND EXISTS(SELECT 1 FROM attachment_events event WHERE event.set_id=candidate.id AND event.company_id=candidate.company_id AND event.action='set.created'
        AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=?)`)
      .bind(actor.companyId,input.targetType,input.targetId,input.componentKind,componentRef,input.purposeKey,input.expectedTargetVersion,actor.userId,Boolean(options.compatibilityAdapter)).first<SetRow>();
    if(reusable)return setDto(tx,reusable);
    await tx.batch([
      tx.prepare(`INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,
        expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,
        required_count,max_files,state,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)`)
        .bind(id,lockedScope.companyId,lockedScope.branchId,lockedScope.departmentId,input.targetType,input.targetId,input.componentKind,componentRef,input.purposeKey,
          input.expectedTargetVersion,policy.version,policy.classification,safeJson({classification:policy.classification,policyVersion:policy.version}),retention.key,retention.version,retention.retainUntil,retention.destructionMode,
          policy.requiredCount,policy.maxFiles,actor.userId,now,now),
      tx.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(`attachment-event-${randomUUID()}`,actor.companyId,id,actor.userId,"set.created",safeJson({targetType:input.targetType,targetId:input.targetId,purposeKey:input.purposeKey,expectedTargetVersion:input.expectedTargetVersion,compatibilityAdapter:Boolean(options.compatibilityAdapter)}),now),
    ]);
    return setDto(tx,(await setRow(tx,id,actor.companyId)));
  });
}

export async function reserveAttachmentFile(db: SqlDatabase, actor: WorkContext, setId: string, input: {filename:string;mediaType:string;bytes:number;caption?:string;supersedesLinkId?:string}, idempotencyKey: string, options:AttachmentServiceOptions={}) {
  requireAttachmentWrite(options); await requireScannerReady(attachmentServiceEnvironment(options));
  if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  const set=await setRow(db,setId,actor.companyId),policy=attachmentPolicy(set.target_type,set.purpose_key,set.component_kind);
  const scope=await authorizeAttachmentTarget(db,actor,policy,set.target_id,"write");
  if(set.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
  if(scope.version!==Number(set.expected_target_version))throw new ApiError(409,"ATTACHMENT_TARGET_CHANGED","The record changed. Create a new file set for its current version.");
  assertAttachmentMedia(policy,input.mediaType,input.bytes);
  const filename=input.filename.trim(),caption=input.caption?.trim()||null;
  if(!filename||filename.length>255||caption&&caption.length>1000)throw new ApiError(422,"ATTACHMENT_METADATA","Check the filename and caption.");
  assertSanitizedEvidenceMetadata(policy,filename,caption);
  if(input.supersedesLinkId){const prior=await db.prepare(`SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND l.state='bound' AND s.company_id=? AND s.target_type=? AND s.target_id=? AND s.purpose_key=?`).bind(input.supersedesLinkId,actor.companyId,set.target_type,set.target_id,set.purpose_key).first();if(!prior)throw new ApiError(422,"ATTACHMENT_SUPERSEDES_INVALID","A correction may supersede only bound evidence on the same record and purpose.");}
  const now=Date.now(),token=stableId("attachment-upload",actor,idempotencyKey),linkId=`attachment-link-${token.slice(token.lastIndexOf("-")+1)}`;
  return atomic(db,async(tx)=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-reserve:${actor.companyId}`).run();
    let currentSet=set;
    if(set.target_type==="leave_request"){
      // Read once to resolve the target, then again after the Leave row lock:
      // an approval that won the lock may have bound this set meanwhile.
      currentSet=await setRow(tx,setId,actor.companyId);
      await lockLeaveAttachmentMutation(tx,actor,{
        targetType:currentSet.target_type,
        targetId:currentSet.target_id,
        purposeKey:currentSet.purpose_key,
        componentKind:currentSet.component_kind,
        expectedTargetVersion:Number(currentSet.expected_target_version),
      });
      currentSet=await setRow(tx,setId,actor.companyId);
      if(currentSet.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
    }
    const replay=await tx.prepare(`SELECT u.attachment_set_id,u.attachment_link_id,u.original_name,u.media_type,u.declared_bytes,u.expires_at,
      l.caption,l.supersedes_link_id FROM upload_intents u JOIN attachment_links l ON l.id=u.attachment_link_id
      WHERE u.id=? AND u.company_id=? AND u.created_by=?`).bind(token,actor.companyId,actor.userId).first<{
        attachment_set_id:string;attachment_link_id:string;original_name:string;media_type:string;declared_bytes:number;expires_at:number;
        caption:string|null;supersedes_link_id:string|null;
      }>();
    if(replay){
      if(replay.attachment_set_id!==setId||replay.original_name!==filename||replay.media_type!==input.mediaType||Number(replay.declared_bytes)!==input.bytes
        ||replay.caption!==caption||replay.supersedes_link_id!==(input.supersedesLinkId??null))
        throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another file reservation.");
      return {uploadIntentId:token,attachmentLinkId:replay.attachment_link_id,uploadUrl:`/api/v1${workspaceBase(publicScope(actor))}/uploads/${token}/content`,expiresAt:Number(replay.expires_at)};
    }
    const count=Number(await tx.prepare("SELECT count(*) AS count FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND s.company_id=? AND l.state IN ('draft','bound')").bind(setId,actor.companyId).first("count"));
    if(count>=Number(currentSet.max_files))throw new ApiError(422,"ATTACHMENT_LIMIT",`This file set accepts at most ${currentSet.max_files} files.`);
    const used=Number(await tx.prepare("SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN ('reserved','stored')").bind(actor.companyId).first("coalesce"));
    const pending=Number(await tx.prepare("SELECT COALESCE(sum(declared_bytes),0) FROM upload_intents WHERE company_id=? AND status='pending' AND expires_at>?").bind(actor.companyId,now).first("coalesce"));
    const limit=Number((await tx.prepare("SELECT storage_bytes FROM beta_policy WHERE company_id=?").bind(actor.companyId).first<{storage_bytes:number}>())?.storage_bytes??Number.MAX_SAFE_INTEGER);
    if(used+pending+input.bytes>limit)throw new ApiError(422,"STORAGE_QUOTA","Company file storage is full.");
    const reserveExpires=now+ATTACHMENT_RESERVATION_MS,draftExpires=reserveExpires,objectKey=`${actor.holdingId}/quarantine/${token}/original`;
    await tx.batch([
      tx.prepare("INSERT INTO attachment_links(id,set_id,caption,state,supersedes_link_id,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,?,?,?)").bind(linkId,setId,caption,input.supersedesLinkId??null,actor.userId,now,draftExpires),
      tx.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,
        owner_version,attachment_set_id,attachment_link_id,company_id,quarantine_key) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
        .bind(token,currentSet.target_type,currentSet.target_id,currentSet.purpose_key,filename,input.mediaType,input.bytes,objectKey,"pending",actor.userId,reserveExpires,now,now,currentSet.expected_target_version,setId,linkId,actor.companyId,objectKey),
      tx.prepare("UPDATE attachment_links SET upload_intent_id=? WHERE id=?").bind(token,linkId),
      tx.prepare("INSERT INTO attachment_events(id,company_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(`attachment-event-${randomUUID()}`,actor.companyId,linkId,setId,actor.userId,"file.reserved",safeJson({filename,mediaType:input.mediaType,bytes:input.bytes}),now),
    ]);
    return {uploadIntentId:token,attachmentLinkId:linkId,uploadUrl:`/api/v1${workspaceBase(publicScope(actor))}/uploads/${token}/content`,expiresAt:reserveExpires};
  });
}

export async function completeAttachmentUpload(db: SqlDatabase, files: FileStore, actor: WorkContext, uploadIntentId: string, buffer: ArrayBuffer, options:AttachmentServiceOptions={}) {
  requireAttachmentWrite(options); await requireScannerReady(attachmentServiceEnvironment(options));
  if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  let intent:Record<string,unknown>|null;
  try {
    intent=await db.prepare(`SELECT * FROM upload_intents WHERE id=? AND attachment_set_id IS NOT NULL AND created_by=? AND company_id=?
      AND (status IN ('uploaded','ready') OR (status='pending' AND expires_at>?))`).bind(uploadIntentId,actor.userId,actor.companyId,Date.now()).first<Record<string,unknown>>();
  } catch(error) { logUploadOperationFailure(options,"load_intent",error); throw error; }
  if(!intent)throw new ApiError(404,"UPLOAD_NOT_FOUND","Upload intent not found or expired.");
  // Replays may follow a completed owner command, but still require the
  // caller's current read grant. New bytes require the original live revision.
  if(intent.owner_type!=='leave_request'){
    const sourceSet=await setRow(db,String(intent.attachment_set_id),actor.companyId),replay=intent.status==='uploaded'||intent.status==='ready';
    const scope=await authorizeAttachmentTarget(db,actor,attachmentPolicy(sourceSet.target_type,sourceSet.purpose_key,sourceSet.component_kind),sourceSet.target_id,replay?'read':'write');
    if(!replay&&(sourceSet.state!=='draft'||scope.version!==Number(intent.owner_version)||scope.version!==Number(sourceSet.expected_target_version)))throw new ApiError(409,'ATTACHMENT_TARGET_CHANGED','The record changed after this file reservation. Create a file set for its current version.');
  }
  const declared=Number(intent.declared_bytes);
  if(buffer.byteLength<1||buffer.byteLength!==declared||buffer.byteLength>10*1024*1024)throw new ApiError(422,"UPLOAD_SIZE","The uploaded bytes must exactly match the reserved size.");
  const bytes=new Uint8Array(buffer),detected=detectedAttachmentMediaType(bytes,String(intent.media_type));
  if(!detected||detected!==intent.media_type)throw new ApiError(422,"UPLOAD_TYPE","The file contents do not match the selected type.");
  const checksum=await sha256(buffer),attachmentId=`attachment-${uploadIntentId}`,objectKey=String(intent.quarantine_key??intent.staging_key),now=Date.now(),leaveIntent=intent.owner_type==="leave_request";
  if((intent.status==="uploaded"||intent.status==="ready")&&!leaveIntent){
    const existing=await db.prepare("SELECT checksum_sha256,source_state FROM attachments WHERE id=? AND company_id=?").bind(attachmentId,actor.companyId).first<{checksum_sha256:string;source_state:AttachmentSourceState}>();
    if(!existing||existing.checksum_sha256!==checksum)throw new ApiError(409,"UPLOAD_CONFLICT","This upload already contains different bytes.");
    return {uploadIntentId,attachmentLinkId:String(intent.attachment_link_id),sourceId:attachmentId,state:existing.source_state,status:"scan_pending" as const};
  }
  let stored: Awaited<ReturnType<FileStore["put"]>> | null = null;
  try {
    stored=intent.status==="pending"
      ? await files.put(objectKey,buffer,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:detected,cacheControl:"private, no-store"},customMetadata:{sha256:checksum,state:"quarantined"}})
      : null;
  } catch (error) {
    logUploadOperationFailure(options,"object_put",error);
    throw error;
  }
  if(intent.status==="pending"&&!stored){
    let prior;
    try { prior=await files.head(objectKey); }
    catch(error) { logUploadOperationFailure(options,"object_head",error); throw error; }
    if(prior?.customMetadata.sha256!==checksum)throw new ApiError(409,"UPLOAD_CONFLICT","This upload key already contains different bytes.");
  }
  try {
    return await atomic(db,async(tx)=>{
      await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-upload:${actor.companyId}:${uploadIntentId}`).run();
      const pendingSet=intent.owner_type==="leave_request"
        ? await setRow(tx,String(intent.attachment_set_id),actor.companyId)
        : null;
      if(pendingSet){
        if(pendingSet.target_type!==intent.owner_type||pendingSet.target_id!==intent.owner_id||pendingSet.purpose_key!==intent.purpose||Number(pendingSet.expected_target_version)!==Number(intent.owner_version))
          throw new ApiError(409,"UPLOAD_CONFLICT","This upload reservation no longer matches its Leave evidence set.");
        // This must precede every intent/link/set row lock so it has the same
        // lock order as the Leave decision transaction.
        await lockLeaveAttachmentTargetRow(tx,actor,pendingSet.target_type,pendingSet.target_id);
      }
      const current=await tx.prepare("SELECT * FROM upload_intents WHERE id=? AND attachment_set_id IS NOT NULL AND created_by=? AND company_id=? FOR UPDATE")
        .bind(uploadIntentId,actor.userId,actor.companyId).first<Record<string,unknown>>();
      if(!current||current.status==="pending"&&Number(current.expires_at)<=now)throw new ApiError(404,"UPLOAD_NOT_FOUND","Upload intent not found or expired.");
      if(current.owner_type==="leave_request"){
        if(!pendingSet||String(current.attachment_set_id)!==pendingSet.id||pendingSet.target_type!==current.owner_type||pendingSet.target_id!==current.owner_id||pendingSet.purpose_key!==current.purpose||Number(pendingSet.expected_target_version)!==Number(current.owner_version))
          throw new ApiError(409,"UPLOAD_CONFLICT","This upload reservation no longer matches its Leave evidence set.");
        await reauthorizeLockedLeaveAttachment(tx,actor,{
          targetType:pendingSet.target_type,
          targetId:pendingSet.target_id,
          purposeKey:pendingSet.purpose_key,
          componentKind:pendingSet.component_kind,
          expectedTargetVersion:Number(pendingSet.expected_target_version),
        },undefined,current.status==="uploaded"||current.status==="ready"?"read":"write");
      }else{
        const sourceSet=await setRow(tx,String(current.attachment_set_id),actor.companyId),replay=current.status==='uploaded'||current.status==='ready',scope=await authorizeAttachmentTarget(tx,actor,attachmentPolicy(sourceSet.target_type,sourceSet.purpose_key,sourceSet.component_kind),sourceSet.target_id,replay?'read':'write');
        if(!replay&&(sourceSet.state!=='draft'||scope.version!==Number(current.owner_version)||scope.version!==Number(sourceSet.expected_target_version)))throw new ApiError(409,'ATTACHMENT_TARGET_CHANGED','The record changed after this file reservation. Create a file set for its current version.');
      }
      if(current.status==="uploaded"||current.status==="ready"){
        const existing=await tx.prepare("SELECT checksum_sha256,source_state FROM attachments WHERE id=? AND company_id=?").bind(attachmentId,actor.companyId).first<{checksum_sha256:string;source_state:AttachmentSourceState}>();
        if(!existing||existing.checksum_sha256!==checksum)throw new ApiError(409,"UPLOAD_CONFLICT","This upload already contains different bytes.");
        if(current.owner_type==="leave_request"){
          const linked=await tx.prepare(`SELECT l.attachment_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
            WHERE l.id=? AND l.set_id=? AND s.company_id=?`).bind(current.attachment_link_id,current.attachment_set_id,actor.companyId).first<{attachment_id:string|null}>();
          if(!linked||linked.attachment_id!==attachmentId)throw new ApiError(409,"UPLOAD_CONFLICT","The completed upload is no longer linked to its reserved Leave evidence slot.");
        }
        return {uploadIntentId,attachmentLinkId:String(current.attachment_link_id),sourceId:attachmentId,state:existing.source_state,status:"scan_pending" as const};
      }
      if(current.owner_type==="leave_request"){
        const lockedLink=await tx.prepare(`SELECT l.state,l.set_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
          WHERE l.id=? AND l.set_id=? AND s.company_id=? FOR UPDATE`)
          .bind(current.attachment_link_id,current.attachment_set_id,actor.companyId).first<{state:string;set_id:string}>();
        if(!lockedLink||lockedLink.state!=="draft")throw new ApiError(409,"ATTACHMENT_LINK_LOCKED","This file link is already bound or closed.");
        const lockedSet=await tx.prepare("SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE")
          .bind(current.attachment_set_id,actor.companyId).first<{state:string}>();
        if(!lockedSet||lockedSet.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
      }
      if(current.status!=="pending")throw new ApiError(409,"UPLOAD_CONFLICT","This upload intent is no longer writable.");
      await tx.batch([
        tx.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,
          company_id,detected_media_type,source_state) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'quarantined')`)
          .bind(attachmentId,uploadIntentId,current.owner_type,current.owner_id,current.purpose,objectKey,current.revision,current.original_name,current.media_type,buffer.byteLength,checksum,actor.userId,now,actor.companyId,detected),
        tx.prepare("UPDATE upload_intents SET status='uploaded',actual_bytes=?,checksum_sha256=?,detected_media_type=?,final_key=?,updated_at=? WHERE id=? AND company_id=? AND status='pending'")
          .bind(buffer.byteLength,checksum,detected,objectKey,now,uploadIntentId,actor.companyId),
        tx.prepare(`UPDATE attachment_links l SET attachment_id=? FROM attachment_sets s WHERE l.id=? AND l.set_id=s.id AND s.company_id=? AND l.state='draft'
          ${current.owner_type==="leave_request"?"AND s.state='draft'":""}`).bind(attachmentId,current.attachment_link_id,actor.companyId),
        tx.prepare("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,1,'pending',?)").bind(`attachment-scan-${randomUUID()}`,attachmentId,now),
        tx.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,?,'stored',?)")
          .bind(`attachment-storage-${randomUUID()}`,actor.companyId,attachmentId,objectKey,buffer.byteLength,now),
        tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
          .bind(`attachment-event-${randomUUID()}`,actor.companyId,attachmentId,current.attachment_link_id,current.attachment_set_id,actor.userId,"file.quarantined",safeJson({checksum,bytes:buffer.byteLength,detectedMediaType:detected}),now),
      ]);
      return {uploadIntentId,attachmentLinkId:String(current.attachment_link_id),sourceId:attachmentId,state:"quarantined" as const,status:"scan_pending" as const};
    });
  } catch(error) {
    logUploadOperationFailure(options,"metadata_transaction",error);
    if(stored){
      // The object store write precedes the database transaction. Serialize
      // cleanup with every retry and delete only when no immutable source won
      // the race and references this key.
      await atomic(db,async cleanupTx=>{
        await cleanupTx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-upload:${actor.companyId}:${uploadIntentId}`).run();
        const referenced=await cleanupTx.prepare("SELECT 1 FROM attachments WHERE company_id=? AND object_key=?").bind(actor.companyId,objectKey).first();
        if(!referenced)await files.delete(objectKey);
      }).catch(()=>undefined);
    }
    throw error;
  }
}

export async function attachmentLinkStatus(db: SqlDatabase, actor: WorkContext, linkId: string) {
  requireAttachmentsV2Read();
  const row=await db.prepare(`SELECT l.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,
    a.source_state,a.id attachment_id,a.preview_object_key FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id
      AND a.company_id=s.company_id
    WHERE l.id=? AND s.company_id=? AND (l.attachment_id IS NULL OR a.id IS NOT NULL)`).bind(linkId,actor.companyId).first<Record<string,unknown>>();
  if(!row)throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  if(attachmentsV2ReadOnly()&&(row.state!=="bound"||row.source_state!=="available"))throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  if(row.state==="draft"&&row.created_by!==actor.userId&&row.target_type!=="leave_request")throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  const policy=attachmentPolicy(String(row.target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind);
  const scope=await authorizeAttachmentTarget(db,actor,policy,String(row.target_id),"read");
  const scan=row.attachment_id?await db.prepare("SELECT attempt,state,result_code,error_code,scanner_signature,scanner_definition_at,completed_at FROM attachment_scan_attempts WHERE attachment_id=? ORDER BY attempt DESC LIMIT 1").bind(row.attachment_id).first<Record<string,unknown>>():null;
  const sourceState=(row.source_state??null) as AttachmentSourceState|null,linkState=row.state as AttachmentLinkState;
  await db.prepare("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) SELECT ?,?,?,?,?, 'status','allowed',? WHERE ?::text IS NOT NULL")
    .bind(`attachment-access-${randomUUID()}`,actor.companyId,row.attachment_id,linkId,actor.userId,Date.now(),row.attachment_id).run();
  return {linkId,setId:String(row.set_id),sourceId:row.attachment_id?String(row.attachment_id):null,sourceState,linkState,
    scan:{attempts:Number(scan?.attempt??0),lastResult:scan?.state??null,lastError:scan?.error_code??null,signature:scan?.scanner_signature??null,definitionUpdatedAt:scan?.scanner_definition_at??null,scannedAt:scan?.completed_at??null},
    previewAvailable:!!row.preview_object_key,bindable:sourceState==="available"&&linkState==="draft"&&(row.target_type==="leave_request"?scope.version>=Number(row.expected_target_version):scope.version===Number(row.expected_target_version)),
    expiresAt:Number(row.expires_at),boundAt:row.bound_at===null?null:Number(row.bound_at),withdrawnAt:row.withdrawn_at===null?null:Number(row.withdrawn_at)};
}

export async function attachmentContent(db: SqlDatabase, files: FileStore, actor: WorkContext, linkId: string, variant: "original"|"preview", options:AttachmentServiceOptions={}) {
  requireAttachmentRead(options);
  const row=await db.prepare(`SELECT l.state link_state,l.created_by link_creator,s.target_type,s.target_id,s.component_kind,s.purpose_key,
    a.id attachment_id,a.object_key,a.preview_object_key,a.preview_media_type,a.original_name,a.media_type,a.source_state
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    WHERE l.id=? AND s.company_id=?`).bind(linkId,actor.companyId).first<Record<string,unknown>>();
  if(!row)throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  try {
    if(attachmentsV2ReadOnly()&&(row.link_state!=="bound"||row.source_state!=="available"))throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    if(row.link_state==="draft"&&row.link_creator!==actor.userId&&row.target_type!=="leave_request")throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    if(row.link_state==="withdrawn"||row.link_state==="expired")throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    await authorizeAttachmentTarget(db,actor,attachmentPolicy(String(row.target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind),String(row.target_id),"read");
    if(row.source_state!=="available")throw new ApiError(423,"ATTACHMENT_NOT_AVAILABLE","The file is still quarantined or was rejected.");
    const key=variant==="preview"?row.preview_object_key:row.object_key;
    if(!key)throw new ApiError(404,"ATTACHMENT_PREVIEW_NOT_FOUND","A safe preview is not available.");
    const object=await files.get(String(key));if(!object)throw new ApiError(503,"FILE_UNAVAILABLE","The private file is temporarily unavailable.");
    await db.prepare("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES(?,?,?,?,?,?, 'allowed',?)")
      .bind(`attachment-access-${randomUUID()}`,actor.companyId,row.attachment_id,linkId,actor.userId,variant,Date.now()).run();
    return {object,mediaType:String(variant==="preview"?row.preview_media_type:row.media_type),filename:String(row.original_name),inline:variant==="preview"};
  } catch(error) {
    await db.prepare("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,reason,created_at) VALUES(?,?,?,?,?,?,'denied',?,?)")
      .bind(`attachment-access-${randomUUID()}`,actor.companyId,row.attachment_id,linkId,actor.userId,variant,error instanceof ApiError?error.code:"ACCESS_DENIED",Date.now()).run().catch(()=>{});
    throw error;
  }
}

export async function withdrawAttachmentLink(db: SqlDatabase, actor: WorkContext, linkId: string, reason: string, idempotencyKey: string) {
  requireAttachmentsV2(); const text=reason.trim();if(text.length<3||text.length>1000)throw new ApiError(422,"WITHDRAWAL_REASON_REQUIRED","Explain why this file is being withdrawn.");
  const row=await db.prepare("SELECT l.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,s.state set_state,s.company_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?").bind(linkId,actor.companyId).first<Record<string,unknown>>();
  if(!row)throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  if(row.state==="bound")throw new ApiError(409,"BOUND_ATTACHMENT_IMMUTABLE","Bound evidence cannot be withdrawn. Add a correction instead.");
  if(row.created_by!==actor.userId)throw new ApiError(403,"ATTACHMENT_WITHDRAW_FORBIDDEN","Only the person who staged this file may withdraw it.");
  await authorizeAttachmentTarget(db,actor,attachmentPolicy(String(row.target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind),String(row.target_id),"write");
  return atomic(db,async tx=>{
    await lockLeaveAttachmentMutation(tx,actor,{
      targetType:String(row.target_type),
      targetId:String(row.target_id),
      purposeKey:String(row.purpose_key),
      componentKind:row.component_kind as AttachmentComponentKind,
      expectedTargetVersion:Number(row.expected_target_version),
    });
    const current=await tx.prepare("SELECT l.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,s.state set_state,s.company_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=? FOR UPDATE")
      .bind(linkId,actor.companyId).first<Record<string,unknown>>();
    if(!current)throw new ApiError(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    if(current.target_type!==row.target_type||current.target_id!==row.target_id||current.purpose_key!==row.purpose_key||Number(current.expected_target_version)!==Number(row.expected_target_version))
      throw new ApiError(409,"ATTACHMENT_LINK_LOCKED","This file link no longer belongs to the reviewed Leave evidence set.");
    if(current.target_type==="leave_request"&&current.set_state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
    if(current.state==="withdrawn"){
      if(current.withdrawn_by!==actor.userId||current.withdrawal_reason!==text)throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This file was withdrawn with a different command.");
      return {id:linkId,state:"withdrawn" as const};
    }
    if(current.state!=="draft")throw new ApiError(409,"ATTACHMENT_LINK_LOCKED","This file link is closed.");
    const now=Date.now();
    await tx.batch([
      tx.prepare("UPDATE attachment_links l SET state='withdrawn',withdrawn_at=?,withdrawn_by=?,withdrawal_reason=? FROM attachment_sets s WHERE l.id=? AND l.set_id=s.id AND s.company_id=? AND l.state='draft'").bind(now,actor.userId,text,linkId,actor.companyId),
      tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING")
        .bind(`attachment-withdraw-${createHash("sha256").update(`${actor.companyId}\0${linkId}\0${idempotencyKey}`).digest("hex").slice(0,32)}`,actor.companyId,current.attachment_id,linkId,current.set_id,actor.userId,"link.withdrawn",safeJson({reason:text}),now),
    ]);
    return {id:linkId,state:"withdrawn" as const};
  });
}

export async function attestAttachmentSetSanitized(
  db: SqlDatabase,
  actor: WorkContext,
  setId: string,
  input: { confirmed: boolean; note: string },
  idempotencyKey: string,
) {
  requireAttachmentsV2();
  const set=await setRow(db,setId,actor.companyId);
  if(!requiresSanitizedEvidenceAttestation(set.target_type))throw new ApiError(422,"ATTACHMENT_ATTESTATION_NOT_APPLICABLE","This file purpose does not use the sanitized-system-evidence review.");
  if(input.confirmed!==true)throw new ApiError(422,"ATTACHMENT_ATTESTATION_REQUIRED","Confirm the sanitized-evidence checklist before continuing.");
  const note=input.note.trim();
  if(note.length<10||note.length>1000)throw new ApiError(422,"ATTACHMENT_ATTESTATION_NOTE","Record a concise review note of 10 to 1,000 characters.");
  const policy=attachmentPolicy(set.target_type,set.purpose_key,set.component_kind);
  await authorizeAttachmentTarget(db,actor,policy,set.target_id,"write");
  if(set.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
  const eventId=stableId("attachment-attestation",actor,idempotencyKey),now=Date.now();
  return atomic(db,async tx=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-attestation:${actor.companyId}:${actor.userId}:${idempotencyKey}`).run();
    const prior=await tx.prepare("SELECT set_id,details_json FROM attachment_events WHERE id=? AND company_id=? AND actor_id=? AND action='sanitization.attested'")
      .bind(eventId,actor.companyId,actor.userId).first<{set_id:string|null;details_json:string}>();
    if(prior?.set_id!==undefined&&prior.set_id!==setId)
      throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another sanitization attestation.");
    const current=await tx.prepare("SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE").bind(setId,actor.companyId).first<{state:string}>();
    if(!current||current.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
    const links=(await tx.prepare(`SELECT l.id,a.id source_id,a.original_name,a.checksum_sha256,a.media_type,a.bytes,a.source_state,l.caption
      FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      WHERE l.set_id=? AND s.company_id=? AND l.state='draft' ORDER BY l.created_at,l.id`).bind(setId,actor.companyId).all<{id:string;source_id:string|null;original_name:string|null;checksum_sha256:string|null;media_type:string|null;bytes:number|null;source_state:AttachmentSourceState|null;caption:string|null}>()).results;
    if(!links.length)throw new ApiError(422,"ATTACHMENT_SET_EMPTY","Upload at least one sanitized review artifact before attesting.");
    if(links.some(link=>link.source_state!=="available"))throw new ApiError(409,"ATTACHMENT_SCAN_PENDING","Every file must pass malware scanning before sanitization review.");
    for(const link of links)assertSanitizedEvidenceMetadata(policy,String(link.original_name),link.caption);
    const details=safeJson({
      note,
      checklist:{noPasswords:true,noPrivateKeys:true,noSecrets:true,noConfigurationDump:true,noDatabaseDump:true,sanitizedReviewArtifactOnly:true},
      manifest:links.map(link=>({linkId:link.id,sourceId:link.source_id,checksumSha256:link.checksum_sha256,mediaType:link.media_type,bytes:Number(link.bytes)})),
    });
    if(prior){
      if(prior.details_json!==details)
        throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used with a different sanitization attestation payload.");
      return setDto(tx,await setRow(tx,setId,actor.companyId));
    }
    await tx.batch([
      tx.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(eventId,actor.companyId,setId,actor.userId,"sanitization.attested",details,now),
      tx.prepare("UPDATE attachment_sets SET updated_at=? WHERE id=? AND company_id=? AND state='draft'").bind(now,setId,actor.companyId),
    ]);
    return setDto(tx,await setRow(tx,setId,actor.companyId));
  });
}

export async function linkExistingAttachment(db: SqlDatabase, actor: WorkContext, setId: string, input: {sourceLinkId:string;caption?:string}, idempotencyKey:string) {
  requireAttachmentsV2();const target=await setRow(db,setId,actor.companyId),targetPolicy=attachmentPolicy(target.target_type,target.purpose_key,target.component_kind);
  await authorizeAttachmentTarget(db,actor,targetPolicy,target.target_id,"write");
  if(target.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","The target file set is closed.");
  const source=await db.prepare(`SELECT l.*,s.company_id,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.classification,s.audience_json,a.source_state,a.media_type,a.bytes
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND s.company_id=? AND a.company_id=s.company_id`).bind(input.sourceLinkId,actor.companyId).first<Record<string,unknown>>();
  if(!source||source.company_id!==target.company_id)throw new ApiError(404,"ATTACHMENT_SOURCE_NOT_FOUND","A reusable file was not found in this Company.");
  if(source.state!=="bound")throw new ApiError(409,"ATTACHMENT_SOURCE_NOT_BOUND","Only immutable bound files can be reused on another record.");
  await authorizeAttachmentTarget(db,actor,attachmentPolicy(String(source.target_type),String(source.purpose_key),source.component_kind as AttachmentComponentKind),String(source.target_id),"read");
  if(source.source_state!=="available")throw new ApiError(409,"ATTACHMENT_SOURCE_UNAVAILABLE","Only clean available sources can be linked.");
  const classificationRank={public:0,internal:1,confidential:2,restricted:3} as const;
  if(classificationRank[target.classification]<classificationRank[String(source.classification) as keyof typeof classificationRank])throw new ApiError(403,"ATTACHMENT_AUDIENCE_EXPANSION","A more sensitive source cannot be reused in a broader file audience.");
  if((targetPolicy.publication||targetPolicy.portalUpload)&&(source.target_type!==target.target_type||source.target_id!==target.target_id))throw new ApiError(403,"ATTACHMENT_AUDIENCE_REUSE_FORBIDDEN","Public or portal-visible media must originate on the same authorized owner.");
  assertAttachmentMedia(targetPolicy,String(source.media_type),Number(source.bytes));
  const id=stableId("attachment-link",actor,idempotencyKey),caption=input.caption?.trim()||null;
  return atomic(db,async tx=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-link:${actor.companyId}:${setId}`).run();
    await lockLeaveAttachmentMutation(tx,actor,{
      targetType:target.target_type,
      targetId:target.target_id,
      purposeKey:target.purpose_key,
      componentKind:target.component_kind,
      expectedTargetVersion:Number(target.expected_target_version),
    },targetPolicy);
    const lockedTarget=await tx.prepare("SELECT * FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE").bind(setId,actor.companyId).first<SetRow>();
    if(!lockedTarget)throw new ApiError(404,"ATTACHMENT_SET_NOT_FOUND","Attachment set not found.");
    if(lockedTarget.target_type!==target.target_type||lockedTarget.target_id!==target.target_id||lockedTarget.purpose_key!==target.purpose_key||Number(lockedTarget.expected_target_version)!==Number(target.expected_target_version))
      throw new ApiError(409,"ATTACHMENT_SET_LOCKED","The target file set changed while it was being reviewed.");
    if(lockedTarget.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","The target file set is closed.");
    const prior=await tx.prepare("SELECT l.set_id,l.attachment_id,l.caption FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?").bind(id,actor.companyId).first<{set_id:string;attachment_id:string;caption:string|null}>();
    if(prior){
      if(prior.set_id!==setId||prior.attachment_id!==source.attachment_id||prior.caption!==caption)throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another file link.");
      return {attachmentLinkId:id,sourceId:String(source.attachment_id)};
    }
    const existing=await tx.prepare("SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND l.attachment_id=? AND s.company_id=? AND l.state IN ('draft','bound')").bind(setId,source.attachment_id,actor.companyId).first<{id:string}>();
    if(existing)return {attachmentLinkId:existing.id,sourceId:String(source.attachment_id)};
    const count=Number(await tx.prepare("SELECT count(*) FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND s.company_id=? AND l.state IN ('draft','bound')").bind(setId,actor.companyId).first("count"));
    if(count>=Number(lockedTarget.max_files))throw new ApiError(422,"ATTACHMENT_LIMIT","The target file set is full.");
    const crossRecord=source.target_type!==target.target_type||source.target_id!==target.target_id;
    const audit={
      sourceLinkId:input.sourceLinkId,
      crossRecord,
      source:{targetType:source.target_type,targetId:source.target_id,purposeKey:source.purpose_key,componentKind:source.component_kind,classification:source.classification,audienceJson:source.audience_json},
      destination:{targetType:target.target_type,targetId:target.target_id,purposeKey:target.purpose_key,componentKind:target.component_kind,classification:target.classification,audienceJson:target.audience_json},
      audienceExpansion:{occurred:crossRecord,classificationWidened:classificationRank[target.classification]<classificationRank[String(source.classification) as keyof typeof classificationRank],addedOwner:crossRecord?{targetType:target.target_type,targetId:target.target_id}:null},
    };
    const now=Date.now();
    await tx.batch([
      tx.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,caption,state,created_by,created_at,expires_at) VALUES(?,?,?,?,'draft',?,?,?)").bind(id,setId,source.attachment_id,caption,actor.userId,now,now+ATTACHMENT_DRAFT_MS),
      tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(`attachment-reuse-${createHash("sha256").update(`${actor.companyId}\0${idempotencyKey}`).digest("hex").slice(0,32)}`,actor.companyId,source.attachment_id,id,setId,actor.userId,"link.reused",safeJson(audit),now),
    ]);
    return {attachmentLinkId:id,sourceId:String(source.attachment_id)};
  });
}

function validateBindingContract(contract:AttachmentBindingContract){
  if(!contract.targetType.trim()||!contract.bindingType.trim()||!contract.contexts.length)throw new ApiError(422,"ATTACHMENT_BINDING_CONTRACT_INVALID","This domain action has no closed attachment binding contract.");
  const seen=new Set<string>();
  for(const context of contract.contexts){
    const purposeKey=context.purposeKey.trim(),componentRef=context.componentRef?.trim()??null,key=`${purposeKey}\0${context.componentKind}\0${componentRef??""}`;
    if(!purposeKey||seen.has(key)||(["action","event","line","message"].includes(context.componentKind)&&!componentRef))throw new ApiError(422,"ATTACHMENT_BINDING_CONTRACT_INVALID","This domain action has an invalid or duplicate attachment context.");
    attachmentPolicy(contract.targetType,purposeKey,context.componentKind);
    seen.add(key);
  }
}

function bindingContextMatches(contract:AttachmentBindingContract,set:SetRow){
  return contract.contexts.some(context=>context.purposeKey===set.purpose_key&&context.componentKind===set.component_kind&&(context.componentRef?.trim()??null)===(set.component_ref?.trim()??null));
}

async function attachmentBindingServiceOptions(db:SqlDatabase,actor:WorkContext,setIds:string[],options:AttachmentServiceOptions){
  if(options.compatibilityAdapter||attachmentsV2Enabled()||!setIds.length)return options;
  const marked=Number(await db.prepare(`SELECT count(DISTINCT candidate.id) FROM attachment_sets candidate
    WHERE candidate.company_id=? AND candidate.id IN (${setIds.map(()=>"?").join(",")})
      AND EXISTS(SELECT 1 FROM attachment_events event WHERE event.set_id=candidate.id AND event.company_id=candidate.company_id
        AND event.action='set.created' AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true)`)
    .bind(actor.companyId,...setIds).first("count"));
  return marked===setIds.length?{compatibilityAdapter:true}:options;
}

export async function prepareAttachmentBinding(db: SqlDatabase, actor: WorkContext, input: {attachmentSetIds:string[];bindingContract:AttachmentBindingContract;targetId:string;expectedTargetVersion:number;commandId:string;bindingId:string},options:AttachmentServiceOptions={}): Promise<PreparedAttachmentBinding> {
  const ids=[...new Set(input.attachmentSetIds)];if(!ids.length||ids.length>20)throw new ApiError(422,"ATTACHMENT_SETS_REQUIRED","Choose one to twenty file sets.");
  options=await attachmentBindingServiceOptions(db,actor,ids,options);requireAttachmentWrite(options);
  if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","File binding is briefly paused for storage reconciliation.");
  validateBindingContract(input.bindingContract);
  const {targetType,bindingType}=input.bindingContract;
  const manifest:AttachmentManifestItem[]=[],guards:SqlStatement[]=[],steps:SqlStatement[]=[],now=Date.now();
  for(const [setIndex,id] of ids.entries()){
    const set=await setRow(db,id,actor.companyId);if(set.target_type!==targetType||set.target_id!==input.targetId)throw new ApiError(422,"ATTACHMENT_SET_TARGET","A file set belongs to another record.");
    if(!bindingContextMatches(input.bindingContract,set))throw new ApiError(422,"ATTACHMENT_BINDING_CONTEXT","Choose only files prepared for this exact domain action and component.");
    if(set.state!=="draft")throw new ApiError(409,"ATTACHMENT_SET_LOCKED","A file set was already bound or closed.");
    const historicalLeave=set.target_type==="leave_request";
    if(historicalLeave?Number(set.expected_target_version)>input.expectedTargetVersion:Number(set.expected_target_version)!==input.expectedTargetVersion)
      throw new ApiError(409,"ATTACHMENT_SET_STALE",historicalLeave?"This Leave file set was authorized against a newer request version.":"Create a file set for the current record version.");
    const policy=attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),scope=await authorizeAttachmentTarget(db,actor,policy,set.target_id,"bind");
    if(scope.version!==input.expectedTargetVersion)throw new ApiError(409,"ATTACHMENT_TARGET_CHANGED","The record changed before evidence was bound.");
    guards.push(attachmentTargetVersionGuard(db,input.commandId,set.target_type,set.target_id,actor,input.expectedTargetVersion,setIndex));
    if(set.target_type==="support_ticket")guards.push(...(await (await import("../support-evidence")).supportEvidenceGuard(db,actor,set.target_id,input.expectedTargetVersion,input.commandId)).guards);
    const links=await db.prepare(`SELECT l.id link_id,l.attachment_id,a.checksum_sha256,a.media_type,a.bytes,a.source_state
      FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      WHERE l.set_id=? AND s.company_id=? AND l.state='draft' ORDER BY l.created_at,l.id`).bind(id,actor.companyId).all<{link_id:string;attachment_id:string|null;checksum_sha256:string|null;media_type:string|null;bytes:number|null;source_state:AttachmentSourceState|null}>();
    if(!links.results.length)throw new ApiError(422,"ATTACHMENT_SET_EMPTY","Bind at least one available file.");
    if(links.results.some(link=>link.source_state!=="available"))throw new ApiError(409,"ATTACHMENT_SCAN_PENDING","Every active file must pass malware scanning before this action.");
    if(links.results.length<Number(set.required_count))throw new ApiError(422,"ATTACHMENT_REQUIRED",`At least ${set.required_count} ${set.purpose_key} file(s) are required.`);
    if(requiresSanitizedEvidenceAttestation(set.target_type)){
      const attested=await db.prepare(`SELECT 1 FROM attachment_events attested WHERE attested.set_id=? AND attested.company_id=? AND attested.action='sanitization.attested'
        AND attested.created_at>=COALESCE((SELECT max(changed.created_at) FROM attachment_events changed WHERE changed.set_id=attested.set_id AND changed.action IN ('file.reserved','link.reused','link.withdrawn')),0)
        ORDER BY attested.created_at DESC LIMIT 1`).bind(id,actor.companyId).first();
      if(!attested)throw new ApiError(422,"ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED","Review the final file list and attest that it contains only sanitized evidence before binding it.");
      guards.push(assertion(db,input.commandId,`attachment-attestation-${setIndex}`,`EXISTS(SELECT 1 FROM attachment_events attested WHERE attested.set_id=? AND attested.company_id=? AND attested.action='sanitization.attested' AND attested.created_at>=COALESCE((SELECT max(changed.created_at) FROM attachment_events changed WHERE changed.set_id=attested.set_id AND changed.action IN ('file.reserved','link.reused','link.withdrawn')),0))`,[id,actor.companyId]));
    }
    guards.push(assertion(db,input.commandId,`attachment-set-${setIndex}`,`EXISTS(SELECT 1 FROM attachment_sets WHERE id=? AND company_id=? AND target_type=? AND target_id=? AND state='draft' AND expected_target_version${historicalLeave?"<=":"="}? AND policy_version=? AND purpose_key=? AND component_kind=? AND component_ref IS NOT DISTINCT FROM ?)`,[id,actor.companyId,targetType,input.targetId,input.expectedTargetVersion,policy.version,set.purpose_key,set.component_kind,set.component_ref]));
    for(const [linkIndex,link] of links.results.entries()){
      guards.push(assertion(db,input.commandId,`attachment-link-${setIndex}-${linkIndex}`,"EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=? AND l.set_id=? AND s.company_id=? AND l.state='draft' AND a.source_state='available' AND a.checksum_sha256=?)",[link.link_id,id,actor.companyId,link.checksum_sha256]));
      manifest.push({linkId:link.link_id,sourceId:String(link.attachment_id),purposeKey:set.purpose_key,checksumSha256:String(link.checksum_sha256),mediaType:String(link.media_type),bytes:Number(link.bytes)});
      steps.push(db.prepare("UPDATE attachment_links l SET state='bound',bound_at=?,binding_type=?,binding_id=? FROM attachment_sets s WHERE l.id=? AND l.set_id=s.id AND s.company_id=? AND l.state='draft'").bind(now,bindingType,input.bindingId,link.link_id,actor.companyId));
    }
    steps.push(db.prepare("UPDATE attachment_sets SET state='bound',bound_at=?,bound_command_id=?,updated_at=? WHERE id=? AND company_id=? AND state='draft'").bind(now,input.commandId,now,id,actor.companyId));
    steps.push(db.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?)")
      .bind(`attachment-bind-${createHash("sha256").update(`${input.commandId}\0${id}`).digest("hex").slice(0,32)}`,actor.companyId,id,actor.userId,"set.bound",safeJson({bindingType,bindingId:input.bindingId,manifest}),now));
  }
  return {manifest,guards,steps};
}

function attachmentTargetVersionGuard(db:SqlDatabase,commandId:string,targetType:string,targetId:string,actor:WorkContext,version:number,index:number){
  const companyId=actor.companyId;
  const portalCollaboration=portalCollaborationAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index);if(portalCollaboration)return portalCollaboration;
  const sensitive=sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index);if(sensitive)return sensitive;
  const financeOperations=financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index);if(financeOperations)return financeOperations;
  const remainingOperational=remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index);if(remainingOperational)return remainingOperational;
  if(targetType==="holding_brand")return assertion(db,commandId,`attachment-owner-${index}`,"?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND kind='company' AND status='active')",[version,companyId]);
  if(targetType==="branch_brand")return assertion(db,commandId,`attachment-owner-${index}`,"?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",[version,targetId,companyId]);
  if(targetType==="company_storefront_brand")return assertion(db,commandId,`attachment-owner-${index}`,"?=1 AND EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND id=? AND kind='company' AND status='active')",[version,targetId,companyId]);
  if(targetType==="crm_opportunity"){
    const ownerEligible=salesRecipientEligibility("l","l.owner_id");
    return assertion(db,commandId,`attachment-owner-${index}`,`EXISTS(SELECT 1 FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
      WHERE o.id=? AND l.company_id=? AND o.version=? AND l.owner_id=?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
        AND ${ownerEligible})`,[targetId,companyId,version,actor.userId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]);
  }
  if(targetType==="crm_account")return assertion(db,commandId,`attachment-owner-${index}`,`EXISTS(SELECT 1 FROM crm_accounts account
    WHERE account.id=? AND account.company_id=? AND account.version=? AND (
      ?::text IS NULL OR account.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead
        WHERE lead.account_id=account.id AND lead.company_id=account.company_id AND lead.branch_id=?
          AND (?::text IS NULL OR lead.department_id=?))))`,
    [targetId,companyId,version,actor.branchId,actor.userId,actor.branchId,actor.departmentId,actor.departmentId]);
  if(targetType==="commerce_fulfillment")return assertion(db,commandId,`attachment-owner-${index}`,"EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=? AND f.version=?)",[targetId,companyId,version]);
  const aliases:Record<string,{table:string;extra?:string}>={
    leave_request:{table:"leave_requests"},purchase_receipt:{table:"commerce_orders",extra:"kind='purchase'"},procurement_rfq:{table:"procurement_rfqs"},purchase_order:{table:"commerce_orders",extra:"kind='purchase'"},supplier_bill:{table:"finance_invoices"},landed_cost:{table:"inventory_landed_costs"},
    stock_count:{table:"inventory_counts"},inventory_stock_count:{table:"inventory_counts"},stock_transfer:{table:"inventory_transfers"},inventory_transfer:{table:"inventory_transfers"},inventory_transit:{table:"inventory_transit"},product:{table:"commerce_products"},inventory_lot:{table:"inventory_lots"},
    quality_hold:{table:"inventory_quality_holds"},inventory_quality_hold:{table:"inventory_quality_holds"},commerce_order:{table:"commerce_orders"},commerce_order_delivery:{table:"commerce_orders"},commerce_return:{table:"commerce_returns"},marketing_campaign:{table:"marketing_campaigns"},
    asset:{table:"operational_assets"},maintenance_work:{table:"maintenance_work_orders"},field_service_visit:{table:"field_service_orders"},production_run:{table:"manufacturing_orders"},planning_scenario:{table:"planning_scenarios"},project:{table:"staff_projects"},support_ticket:{table:"support_tickets"},employee_request:{table:"employee_requests"},expense_claim:{table:"finance_expenses"},expense_payment:{table:"expense_payment_drafts"},invoice_payment:{table:"finance_invoices"},staff_task:{table:"staff_tasks"},social_draft:{table:"social_post_drafts"},messaging_room:{table:"messaging_rooms"},legal_item:{table:"legal_items"},
  };const item=aliases[targetType];if(!item)throw new ApiError(422,"ATTACHMENT_TARGET_UNSUPPORTED","This file target has no atomic version guard.");
  return assertion(db,commandId,`attachment-owner-${index}`,`EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?${item.extra?` AND ${item.extra}`:""})`,[targetId,companyId,version]);
}

const recordVersionTables:Record<string,{table:string;extra?:string}>={
  procurement_rfq:{table:"procurement_rfqs"},purchase_order:{table:"commerce_orders",extra:"kind='purchase'"},supplier_bill:{table:"finance_invoices"},landed_cost:{table:"inventory_landed_costs"},
  stock_count:{table:"inventory_counts"},inventory_stock_count:{table:"inventory_counts"},stock_transfer:{table:"inventory_transfers"},inventory_transfer:{table:"inventory_transfers"},product:{table:"commerce_products"},inventory_lot:{table:"inventory_lots"},
  inventory_transit:{table:"inventory_transit"},
  quality_hold:{table:"inventory_quality_holds"},inventory_quality_hold:{table:"inventory_quality_holds"},commerce_order:{table:"commerce_orders"},commerce_order_delivery:{table:"commerce_orders"},commerce_return:{table:"commerce_returns"},crm_account:{table:"crm_accounts"},crm_opportunity:{table:"crm_opportunities"},
  marketing_campaign:{table:"marketing_campaigns"},asset:{table:"operational_assets"},maintenance_work:{table:"maintenance_work_orders"},field_service_visit:{table:"field_service_orders"},production_run:{table:"manufacturing_orders"},planning_scenario:{table:"planning_scenarios"},project:{table:"staff_projects"},
  support_ticket:{table:"support_tickets"},employee_request:{table:"employee_requests"},expense_claim:{table:"finance_expenses"},expense_payment:{table:"expense_payment_drafts"},invoice_payment:{table:"finance_invoices"},staff_task:{table:"staff_tasks"},social_draft:{table:"social_post_drafts"},
};
export function attachmentRecordVersionStep(db:SqlDatabase,targetType:string,targetId:string,expectedVersion:number,now=Date.now()){
  const portalCollaboration=portalCollaborationAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now);if(portalCollaboration)return portalCollaboration;
  const sensitive=sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now);if(sensitive)return sensitive;
  const financeOperations=financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now);if(financeOperations)return financeOperations;
  const remainingOperational=remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now);if(remainingOperational)return remainingOperational;
  if(targetType==="leave_request")return db.prepare("UPDATE leave_requests SET version=version+1,evidence_version=evidence_version+1,updated_at=? WHERE id=? AND version=?").bind(now,targetId,expectedVersion);
  const descriptor=recordVersionTables[targetType];if(!descriptor)throw new ApiError(422,"RECORD_ATTACHMENT_BIND_UNSUPPORTED","This record does not support standalone document binding.");
  return db.prepare(`UPDATE ${descriptor.table} SET version=version+1${["inventory_lots","commerce_products","marketing_campaigns"].includes(descriptor.table)?"":",updated_at=?"} WHERE id=? AND version=?${descriptor.extra?` AND ${descriptor.extra}`:""}`)
    .bind(...(["inventory_lots","commerce_products","marketing_campaigns"].includes(descriptor.table)?[targetId,expectedVersion]:[now,targetId,expectedVersion]));
}

export async function bindRecordAttachmentSet(db:SqlDatabase,actor:WorkContext,setId:string,expectedTargetVersion:number,commandId:string,options:AttachmentServiceOptions={}){
  requireAttachmentWrite(options);
  const set=await setRow(db,setId,actor.companyId),brandSource=isBrandPublicationTarget(set.target_type)&&set.component_kind==="publication";
  if(set.component_kind!=="record"&&!brandSource)throw new ApiError(422,"ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED","Action, line and event evidence must bind inside its domain command.");
  if(Number(set.expected_target_version)!==expectedTargetVersion)throw new ApiError(409,"ATTACHMENT_SET_STALE","Create a record-document set for the current record version.");
  const operation=brandSource?"attachment.publication-source.bind":"attachment.record.bind",nextVersion=brandSource?expectedTargetVersion:expectedTargetVersion+1,prior=await replayCommand(db,actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(prior){await authorizeAttachmentTarget(db,actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,'read');return prior;}
  const bindingType=brandSource?"publication_source":"record_document",prepared=await prepareAttachmentBinding(db,actor,{attachmentSetIds:[setId],bindingContract:{targetType:set.target_type,bindingType,contexts:[{purposeKey:set.purpose_key,componentKind:set.component_kind,componentRef:set.component_ref}]},targetId:set.target_id,expectedTargetVersion,commandId,bindingId:set.target_id},options);
  return executeCommand(db,actor,commandId,operation,{setId,expectedTargetVersion},set.target_id,nextVersion,{setId,version:nextVersion,manifest:prepared.manifest},[
    ...prepared.guards,...(brandSource?[]:[attachmentRecordVersionStep(db,set.target_type,set.target_id,expectedTargetVersion)]),...prepared.steps,
  ],{companyId:set.company_id,branchId:set.branch_id,departmentId:set.department_id});
}

/** One-release adapter for callers that historically treated upload completion
 * as the owner mutation. New workflows must bind action evidence in their own
 * domain command instead. */
export async function bindCompatibilityAttachmentSet(db:SqlDatabase,actor:WorkContext,setId:string,expectedTargetVersion:number,commandId:string,options:AttachmentServiceOptions={}){
  requireAttachmentWrite(options);
  const set=await setRow(db,setId,actor.companyId),brandSource=isBrandPublicationTarget(set.target_type)&&set.component_kind==="publication";
  if(set.component_kind==="publication"&&!brandSource)throw new ApiError(422,"PUBLICATION_SOURCE_UNSUPPORTED","This publication source requires an explicit domain binding workflow.");
  const operation=brandSource?"attachment.publication-source.compatibility-bind":"attachment.compatibility.bind",nextVersion=brandSource?expectedTargetVersion:expectedTargetVersion+1,prior=await replayCommand(db,actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(prior){await authorizeAttachmentTarget(db,actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,'read');return prior;}
  const bindingType=brandSource?"publication_source":"legacy_upload_adapter",prepared=await prepareAttachmentBinding(db,actor,{attachmentSetIds:[setId],bindingContract:{targetType:set.target_type,bindingType,contexts:[{purposeKey:set.purpose_key,componentKind:set.component_kind,componentRef:set.component_ref}]},targetId:set.target_id,expectedTargetVersion,commandId,bindingId:set.target_id},options);
  const result={id:prepared.manifest[0]!.sourceId,ownerVersion:nextVersion,attachmentSetId:setId,attachmentLinkId:prepared.manifest[0]!.linkId,state:"available" as const,publicationReviewRequired:brandSource};
  const compatibilityEvents=set.target_type==="support_ticket"?[(await import("../support-evidence")).supportEvidenceEvent(db,set.target_id,commandId,actor.userId,"attachment-add",{attachmentId:result.id,manifest:prepared.manifest},"Attached private support evidence")]:[];
  return executeCommand(db,actor,commandId,operation,{setId,expectedTargetVersion},set.target_id,nextVersion,result,[
    ...prepared.guards,...(brandSource?[]:[attachmentRecordVersionStep(db,set.target_type,set.target_id,expectedTargetVersion)]),...prepared.steps,...compatibilityEvents,
  ],{companyId:set.company_id,branchId:set.branch_id,departmentId:set.department_id});
}

function brandPublicationApprovalStep(db:SqlDatabase,row:Record<string,unknown>,publicationId:string,actor:WorkContext,now:number):SqlStatement|null{
  const targetType=String(row.source_target_type),targetId=String(row.source_target_id),purpose=String(row.purpose_key);
  if(targetType==="holding_brand"||targetType==="branch_brand"){
    const kind=targetType==="holding_brand"?"holding":"branch",column=purpose==="inverse"?"inverse_publication_id":"primary_publication_id";
    return db.prepare(`INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at,${column}) VALUES(?,?,?,?,?,?,?)
      ON CONFLICT(scope_kind,scope_id) DO UPDATE SET ${column}=excluded.${column},updated_by=excluded.updated_by,updated_at=excluded.updated_at`)
      .bind(kind,targetId,kind==="branch"?actor.companyId:null,kind==="holding"?"holding":"branch",actor.userId,now,publicationId);
  }
  if(targetType==="company_storefront_brand")return db.prepare(`INSERT INTO storefront_app_brands(company_id,short_name,accent_color,updated_by,updated_at,logo_publication_id)
    SELECT id,left(display_name,40),'#3659c9',?,?,? FROM organization_nodes WHERE id=? AND kind='company' AND status='active'
    ON CONFLICT(company_id) DO UPDATE SET logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at`)
    .bind(actor.userId,now,publicationId,targetId);
  return null;
}

function brandPublicationRevocationStep(db:SqlDatabase,row:Record<string,unknown>,publicationId:string):SqlStatement|null{
  const targetType=String(row.source_target_type),targetId=String(row.source_target_id),purpose=String(row.purpose_key);
  if(targetType==="holding_brand"||targetType==="branch_brand"){
    const column=purpose==="inverse"?"inverse_publication_id":"primary_publication_id";
    return db.prepare(`UPDATE organization_brand_profiles SET ${column}=NULL WHERE scope_kind=? AND scope_id=? AND ${column}=?`)
      .bind(targetType==="holding_brand"?"holding":"branch",targetId,publicationId);
  }
  if(targetType==="company_storefront_brand")return db.prepare("UPDATE storefront_app_brands SET logo_publication_id=NULL WHERE company_id=? AND logo_publication_id=?").bind(targetId,publicationId);
  return null;
}

export async function createAttachmentPublication(db:SqlDatabase,files:FileStore,actor:WorkContext,input:{sourceLinkId:string;targetType:string;targetId:string;altText:string;ordinal:number},idempotencyKey:string){
  requireAttachmentsV2();if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","File intake and publication are briefly paused for storage reconciliation.");const alt=input.altText.trim();if(!alt||alt.length>500||!Number.isSafeInteger(input.ordinal)||input.ordinal<0||input.ordinal>10_000)throw new ApiError(422,"PUBLICATION_METADATA","Provide concise alt text and a valid display order.");
  const source=await db.prepare(`SELECT l.id link_id,l.attachment_id,s.target_type source_target_type,s.target_id source_target_id,s.component_kind,s.purpose_key,s.company_id,
    a.preview_object_key,a.preview_media_type,a.preview_checksum_sha256,a.preview_bytes,a.source_state
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    WHERE l.id=? AND s.company_id=? AND l.state='bound'`)
    .bind(input.sourceLinkId,actor.companyId).first<Record<string,unknown>>();
  if(!source||source.source_state!=="available"||!source.preview_object_key)throw new ApiError(404,"PUBLICATION_SOURCE_NOT_FOUND","Choose an available image with a sanitized preview.");
  const policy=attachmentPolicy(String(source.source_target_type),String(source.purpose_key),source.component_kind as AttachmentComponentKind);if(!policy.publication)throw new ApiError(422,"PUBLICATION_NOT_ALLOWED","This file purpose cannot be published.");
  await authorizeAttachmentTarget(db,actor,policy,String(source.source_target_id),"publish");
  if(input.targetType!==source.source_target_type||input.targetId!==source.source_target_id)throw new ApiError(422,"PUBLICATION_TARGET_MISMATCH","Published media must remain attached to its authorized source owner.");
  const id=stableId("media-publication",actor,idempotencyKey),existing=await db.prepare("SELECT * FROM attachment_publications WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Record<string,unknown>>();
  if(existing){if(existing.source_link_id!==input.sourceLinkId||existing.target_type!==input.targetType||existing.target_id!==input.targetId||existing.alt_text!==alt||Number(existing.ordinal)!==input.ordinal)throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another publication.");return existing;}
  const original=await files.get(String(source.preview_object_key));if(!original)throw new ApiError(503,"FILE_UNAVAILABLE","The sanitized media is temporarily unavailable.");const bytes=await original.arrayBuffer(),checksum=await sha256(bytes),now=Date.now();
  return atomic(db,async tx=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-publication:${actor.companyId}:${input.targetType}:${input.targetId}`).run();
    const replay=await tx.prepare("SELECT * FROM attachment_publications WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Record<string,unknown>>();if(replay)return replay;
    const revision=Number(await tx.prepare("SELECT COALESCE(max(revision),0)+1 AS next_revision FROM attachment_publications WHERE company_id=? AND target_type=? AND target_id=?").bind(actor.companyId,input.targetType,input.targetId).first("next_revision")),key=`${actor.holdingId}/published/${id}/r${revision}`;
    await files.put(key,bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:String(source.preview_media_type),cacheControl:"no-store"},customMetadata:{sha256:checksum,sanitized:"true"}});
    await tx.batch([
      tx.prepare(`INSERT INTO attachment_publications(id,company_id,source_link_id,target_type,target_id,derivative_object_key,derivative_media_type,derivative_checksum_sha256,
        derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?, 'draft',?,?)`).bind(id,actor.companyId,input.sourceLinkId,input.targetType,input.targetId,key,source.preview_media_type,checksum,bytes.byteLength,alt,input.ordinal,revision,actor.userId,now),
      tx.prepare("INSERT INTO attachment_publication_orders(company_id,target_type,target_id,version,updated_by,updated_at) VALUES(?,?,?,1,?,?) ON CONFLICT(company_id,target_type,target_id) DO NOTHING").bind(actor.companyId,input.targetType,input.targetId,actor.userId,now),
      tx.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'publication',?,?,'stored',?)").bind(`attachment-storage-${randomUUID()}`,actor.companyId,source.attachment_id,key,bytes.byteLength,now),
      tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`attachment-event-${randomUUID()}`,actor.companyId,source.attachment_id,input.sourceLinkId,actor.userId,"publication.created",safeJson({publicationId:id,targetType:input.targetType,targetId:input.targetId,revision}),now),
    ]);return tx.prepare("SELECT * FROM attachment_publications WHERE id=?").bind(id).first<Record<string,unknown>>();
  });
}

async function publicationTargetPolicy(db:SqlDatabase,actor:WorkContext,targetType:string,targetId:string,operation:"read"|"publish"){
  const policy=attachmentPolicyRegistry.find(item=>item.targetType===targetType&&item.publication);if(!policy)throw new ApiError(422,"PUBLICATION_TARGET_UNSUPPORTED","This target does not publish media.");await authorizeAttachmentTarget(db,actor,policy,targetId,operation);return policy;
}
export async function listAttachmentPublications(db:SqlDatabase,actor:WorkContext,targetType:string,targetId:string){
  requireAttachmentsV2Read();await publicationTargetPolicy(db,actor,targetType,targetId,"read");let canManage=!attachmentsV2ReadOnly();if(canManage)try{await publicationTargetPolicy(db,actor,targetType,targetId,"publish");}catch(error){if(!(error instanceof ApiError)||error.status!==403)throw error;canManage=false;}const rows=(await db.prepare(`SELECT id,source_link_id,target_type,target_id,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at,approved_by,approved_at,revoked_by,revoked_at,revocation_reason FROM attachment_publications publication WHERE company_id=? AND target_type=? AND target_id=?
    AND EXISTS(SELECT 1 FROM attachment_links link JOIN attachment_sets set_row ON set_row.id=link.set_id WHERE link.id=publication.source_link_id AND set_row.company_id=publication.company_id)
    ${canManage?"":"AND state='approved'"} ORDER BY ordinal,revision,id`).bind(actor.companyId,targetType,targetId).all<Record<string,unknown>>()).results,order=await db.prepare("SELECT version FROM attachment_publication_orders WHERE company_id=? AND target_type=? AND target_id=?").bind(actor.companyId,targetType,targetId).first<{version:number}>();return {targetType,targetId,orderVersion:Number(order?.version??1),items:rows};
}
export async function revokeAttachmentPublication(db:SqlDatabase,actor:WorkContext,id:string,reason:string,idempotencyKey:string){
  requireAttachmentsV2();
  const text=reason.trim();
  if(text.length<3||text.length>1000)throw new ApiError(422,"PUBLICATION_REVOCATION_REASON","Explain why this media is being revoked.");
  const row=await db.prepare(`SELECT p.*,s.component_kind,s.purpose_key,s.target_type source_target_type,s.target_id source_target_id
    FROM attachment_publications p JOIN attachment_links l ON l.id=p.source_link_id JOIN attachment_sets s ON s.id=l.set_id
      AND s.company_id=p.company_id
    WHERE p.id=? AND p.company_id=?`).bind(id,actor.companyId).first<Record<string,unknown>>();
  if(!row)throw new ApiError(404,"PUBLICATION_NOT_FOUND","Media publication not found.");
  const policy=attachmentPolicy(String(row.source_target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind);
  await authorizeAttachmentTarget(db,actor,policy,String(row.source_target_id),"publish");
  if(!hasPermission(actor,"records.review")&&!hasPermission(actor,"commerce.approve"))throw new ApiError(403,"PUBLICATION_APPROVER_REQUIRED","An authorized reviewer must revoke public media.");
  return atomic(db,async tx=>{
    const current=await tx.prepare("SELECT * FROM attachment_publications WHERE id=? AND company_id=? FOR UPDATE").bind(id,actor.companyId).first<Record<string,unknown>>();
    if(!current)throw new ApiError(404,"PUBLICATION_NOT_FOUND","Media publication not found.");
    if(current.state==="revoked"){
      if(current.revocation_reason!==text)throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This publication was revoked for a different reason.");
      return current;
    }
    if(current.state!=="approved")throw new ApiError(409,"PUBLICATION_NOT_APPROVED","Only approved public media can be revoked.");
    const now=Date.now();
    const pointerStep=brandPublicationRevocationStep(tx,row,id);
    await tx.batch([
      tx.prepare("UPDATE attachment_publications SET state='revoked',revoked_by=?,revoked_at=?,revocation_reason=? WHERE id=? AND company_id=? AND state='approved'").bind(actor.userId,now,text,id,actor.companyId),
      ...(pointerStep?[pointerStep]:[]),
      tx.prepare("INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING").bind(`publication-revoke-${createHash("sha256").update(`${actor.companyId}\0${id}\0${idempotencyKey}`).digest("hex").slice(0,32)}`,actor.companyId,row.source_link_id,actor.userId,"publication.revoked",safeJson({publicationId:id,reason:text}),now),
    ]);
    return tx.prepare("SELECT * FROM attachment_publications WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Record<string,unknown>>();
  });
}
export async function reorderAttachmentPublications(db:SqlDatabase,actor:WorkContext,input:{targetType:string;targetId:string;expectedVersion:number;items:Array<{id:string;ordinal:number}>},idempotencyKey:string){
  requireAttachmentsV2();
  await publicationTargetPolicy(db,actor,input.targetType,input.targetId,"publish");
  if(!Number.isSafeInteger(input.expectedVersion)||input.expectedVersion<1||!input.items.length||input.items.length>100
    ||new Set(input.items.map(item=>item.id)).size!==input.items.length
    ||input.items.some(item=>!Number.isSafeInteger(item.ordinal)||item.ordinal<0||item.ordinal>10_000))
    throw new ApiError(422,"PUBLICATION_ORDER_INVALID","Provide each active publication once with a valid display order.");
  const request={targetType:input.targetType,targetId:input.targetId,expectedVersion:input.expectedVersion,items:[...input.items].sort((left,right)=>left.id.localeCompare(right.id))};
  const requestJson=safeJson(request),requestHash=createHash("sha256").update(requestJson).digest("hex"),eventId=stableId("publication-order",actor,idempotencyKey);
  return atomic(db,async tx=>{
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`publication-order:${actor.companyId}:${actor.userId}:${idempotencyKey}`).run();
    const replay=await tx.prepare("SELECT details_json FROM attachment_events WHERE id=? AND company_id=? AND actor_id=? AND action='publication.reordered'")
      .bind(eventId,actor.companyId,actor.userId).first<{details_json:string}>();
    if(replay){
      const details=JSON.parse(replay.details_json) as {requestHash?:string;result?:{targetType:string;targetId:string;orderVersion:number}};
      if(details.requestHash!==requestHash||!details.result)
        throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another public-media order.");
      return details.result;
    }
    const order=await tx.prepare("SELECT version FROM attachment_publication_orders WHERE company_id=? AND target_type=? AND target_id=? FOR UPDATE")
      .bind(actor.companyId,input.targetType,input.targetId).first<{version:number}>();
    const current=Number(order?.version??1);
    if(current!==input.expectedVersion)throw new ApiError(409,"PUBLICATION_ORDER_CHANGED","Public media order changed. Refresh and retry.");
    const active=(await tx.prepare("SELECT id FROM attachment_publications WHERE company_id=? AND target_type=? AND target_id=? AND state<>'revoked' ORDER BY id")
      .bind(actor.companyId,input.targetType,input.targetId).all<{id:string}>()).results.map(row=>row.id).sort();
    const submitted=input.items.map(item=>item.id).sort();
    if(active.length!==submitted.length||active.some((id,index)=>id!==submitted[index]))
      throw new ApiError(409,"PUBLICATION_SET_CHANGED","Public media changed. Refresh and retry.");
    const now=Date.now(),result={targetType:input.targetType,targetId:input.targetId,orderVersion:current+1};
    await tx.batch([
      ...input.items.map(item=>tx.prepare("UPDATE attachment_publications SET ordinal=? WHERE id=? AND company_id=?").bind(item.ordinal,item.id,actor.companyId)),
      tx.prepare("INSERT INTO attachment_publication_orders(company_id,target_type,target_id,version,updated_by,updated_at) VALUES(?,?,?,2,?,?) ON CONFLICT(company_id,target_type,target_id) DO UPDATE SET version=attachment_publication_orders.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at WHERE attachment_publication_orders.version=?")
        .bind(actor.companyId,input.targetType,input.targetId,actor.userId,now,input.expectedVersion),
      tx.prepare("INSERT INTO attachment_events(id,company_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?)")
        .bind(eventId,actor.companyId,actor.userId,"publication.reordered",safeJson({requestHash,request,result}),now),
    ]);
    return result;
  });
}

export async function approveAttachmentPublication(db:SqlDatabase,actor:WorkContext,id:string,idempotencyKey:string){
  requireAttachmentsV2();const row=await db.prepare(`SELECT p.*,s.target_type source_target_type,s.target_id source_target_id,s.component_kind,s.purpose_key
    FROM attachment_publications p JOIN attachment_links l ON l.id=p.source_link_id JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=p.company_id
    WHERE p.id=? AND p.company_id=?`).bind(id,actor.companyId).first<Record<string,unknown>>();
  if(!row)throw new ApiError(404,"PUBLICATION_NOT_FOUND","Media publication not found.");
  if(row.created_by===actor.userId)throw new ApiError(403,"INDEPENDENT_PUBLICATION_REVIEW","Another authorized person must approve public media.");
  if(!hasPermission(actor,"records.review")&&!hasPermission(actor,"commerce.approve"))throw new ApiError(403,"PUBLICATION_APPROVER_REQUIRED","An authorized reviewer must approve public media.");
  const policy=attachmentPolicy(String(row.source_target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind);await authorizeAttachmentTarget(db,actor,policy,String(row.source_target_id),"publish");if(row.state==="approved")return row;if(row.state!=="draft")throw new ApiError(409,"PUBLICATION_LOCKED","This publication can no longer be approved.");const now=Date.now();
  return atomic(db,async tx=>{const current=await tx.prepare("SELECT state FROM attachment_publications WHERE id=? AND company_id=? FOR UPDATE").bind(id,actor.companyId).first<{state:string}>();if(current?.state==="approved")return tx.prepare("SELECT * FROM attachment_publications WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Record<string,unknown>>();if(current?.state!=="draft")throw new ApiError(409,"PUBLICATION_LOCKED","This publication can no longer be approved.");const pointerStep=brandPublicationApprovalStep(tx,row,id,actor,now);await tx.batch([
    tx.prepare("UPDATE attachment_publications SET state='approved',approved_by=?,approved_at=? WHERE id=? AND company_id=? AND state='draft'").bind(actor.userId,now,id,actor.companyId),
    ...(pointerStep?[pointerStep]:[]),
    tx.prepare("INSERT INTO attachment_events(id,company_id,link_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING")
      .bind(`publication-approval-${createHash("sha256").update(`${actor.companyId}\0${id}\0${idempotencyKey}`).digest("hex").slice(0,32)}`,actor.companyId,row.source_link_id,actor.userId,"publication.approved",safeJson({publicationId:id,revision:row.revision}),now),
  ]);return tx.prepare("SELECT * FROM attachment_publications WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Record<string,unknown>>();});
}

export async function publicAttachmentPublication(db:SqlDatabase,files:FileStore,id:string){
  const row=await db.prepare("SELECT derivative_object_key,derivative_media_type,derivative_checksum_sha256,alt_text,revision FROM attachment_publications WHERE id=? AND state='approved'").bind(id).first<{derivative_object_key:string;derivative_media_type:string;derivative_checksum_sha256:string;alt_text:string;revision:number}>();
  if(!row)throw new ApiError(404,"PUBLICATION_NOT_FOUND","Published media not found.");const object=await files.get(row.derivative_object_key);if(!object)throw new ApiError(503,"FILE_UNAVAILABLE","Published media is temporarily unavailable.");return {row,object};
}

export async function drainAttachmentScans(db:SqlDatabase,files:FileStore,attachmentId?:string,options:{offlineMigration?:boolean}={}){
  if(options.offlineMigration&&!attachmentUploadsFrozen())throw new Error("OFFLINE_ATTACHMENT_SCAN_REQUIRES_UPLOAD_FREEZE");
  const scannerEnvironment=!attachmentsV2Enabled()?{...process.env,ERPX_ATTACHMENTS_V2:"true"}:process.env;
  await requireScannerReady(scannerEnvironment);const now=Date.now(),lease=`attachment-scan-lease-${randomUUID()}`;
  const job=await db.prepare(`UPDATE attachment_scan_attempts SET state='running',claimed_at=?,claim_until=?,lease_token=? WHERE id=(
    SELECT id FROM attachment_scan_attempts WHERE state IN ('pending','running') AND created_at<=? AND (claim_until IS NULL OR claim_until<?) AND (?::text IS NULL OR attachment_id=?) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1)
    RETURNING *`).bind(now,now+60_000,lease,now,now,attachmentId??null,attachmentId??null).first<Record<string,unknown>>();if(!job)return false;
  const source=await db.prepare("SELECT * FROM attachments WHERE id=?").bind(job.attachment_id).first<Record<string,unknown>>();if(!source)throw new Error("ATTACHMENT_SCAN_SOURCE_MISSING");
  await db.prepare("UPDATE attachments SET source_state='scanning' WHERE id=? AND source_state IN ('quarantined','scan_error')").bind(source.id).run();
  try{
    const stored=await files.get(String(source.object_key));if(!stored)throw new Error("ATTACHMENT_OBJECT_MISSING");const originalBuffer=await stored.arrayBuffer(),bytes=new Uint8Array(originalBuffer);
    if(bytes.byteLength>10*1024*1024)throw new Error("ATTACHMENT_SCAN_OVERSIZE");
    if(await sha256(originalBuffer)!==source.checksum_sha256)throw new Error("ATTACHMENT_CHECKSUM_MISMATCH");
    const detected=detectedAttachmentMediaType(bytes,String(source.media_type));
    if(!detected||detected!==source.media_type)throw new Error("ATTACHMENT_MEDIA_MISMATCH");
    const scanned=await scanAttachmentBytes(bytes,scannerEnvironment);
    if(!scanned.clean){await db.batch([
      db.prepare("UPDATE attachment_scan_attempts SET state='infected',scanner_engine=?,scanner_signature=?,scanner_definition_at=?,result_code='FOUND',completed_at=?,claim_until=NULL WHERE id=? AND lease_token=?").bind(scanned.engine,scanned.signature,scanned.definitionUpdatedAt,Date.now(),job.id,lease),
      db.prepare("UPDATE attachments SET source_state='malware_rejected',detected_media_type=COALESCE(detected_media_type,?),scanned_at=?,scanner_definition_at=? WHERE id=?").bind(detected,Date.now(),scanned.definitionUpdatedAt,source.id),
      db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?)").bind(`attachment-event-${randomUUID()}`,source.company_id,source.id,"system:jobs","scan.malware_rejected",safeJson({signature:scanned.signature,engine:scanned.engine}),Date.now()),
    ]);return true;}
    const preview=await sanitizedAttachmentPreview(bytes,String(source.media_type)),previewBuffer=preview?preview.bytes.slice().buffer:null,previewKey=preview&&previewBuffer?`${String(source.object_key).replace(/\/original$/,"")}/preview-${await sha256(previewBuffer)}.${preview.mediaType==="image/webp"?"webp":"jpg"}`:null;
    if(preview&&previewKey)await files.put(previewKey,preview.bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:preview.mediaType,cacheControl:"private, no-store"},customMetadata:{sourceSha256:String(source.checksum_sha256),sanitized:"true"}});
    const previewChecksum=previewBuffer?await sha256(previewBuffer):null,completed=Date.now(),statements:SqlStatement[]=[
      db.prepare("UPDATE attachment_scan_attempts SET state='clean',scanner_engine=?,scanner_signature=NULL,scanner_definition_at=?,result_code='OK',completed_at=?,claim_until=NULL WHERE id=? AND lease_token=?").bind(scanned.engine,scanned.definitionUpdatedAt,completed,job.id,lease),
      db.prepare("UPDATE attachments SET source_state='available',detected_media_type=COALESCE(detected_media_type,?),scanned_at=?,scanner_definition_at=?,preview_object_key=?,preview_media_type=?,preview_checksum_sha256=?,preview_bytes=? WHERE id=?").bind(detected,completed,scanned.definitionUpdatedAt,previewKey,preview?.mediaType??null,previewChecksum,preview?.bytes.byteLength??null,source.id),
      db.prepare("UPDATE upload_intents SET status='ready',updated_at=? WHERE id=?").bind(completed,source.upload_id),
      db.prepare("UPDATE attachment_links SET expires_at=? WHERE upload_intent_id=? AND state='draft'").bind(completed+ATTACHMENT_DRAFT_MS,source.upload_id),
      db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?)").bind(`attachment-event-${randomUUID()}`,source.company_id,source.id,"system:jobs","scan.available",safeJson({engine:scanned.engine,definitionUpdatedAt:scanned.definitionUpdatedAt,preview:!!preview}),completed),
    ];if(preview&&previewKey)statements.push(db.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'preview',?,?,'stored',?)").bind(`attachment-storage-${randomUUID()}`,source.company_id,source.id,previewKey,preview.bytes.byteLength,completed));
    await db.batch(statements);return true;
  }catch(error){const attempt=Number(job.attempt),completed=Date.now(),code=(error instanceof Error?error.message:"SCAN_ERROR").slice(0,160),steps=[
    db.prepare("UPDATE attachment_scan_attempts SET state='error',error_code=?,completed_at=?,claim_until=NULL WHERE id=? AND lease_token=?").bind(code,completed,job.id,lease),
    db.prepare("UPDATE attachments SET source_state='scan_error' WHERE id=?").bind(source.id),
    db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?)").bind(`attachment-event-${randomUUID()}`,source.company_id,source.id,"system:jobs","scan.error",safeJson({attempt,code}),completed),
  ];if(attempt<6)steps.push(db.prepare("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,?,'pending',?)").bind(`attachment-scan-${randomUUID()}`,source.id,attempt+1,completed+Math.min(60_000,1000*2**attempt)));await db.batch(steps);return true;}
}

export async function scanAttachmentNow(db:SqlDatabase,files:FileStore,attachmentId:string){
  if(!attachmentId)throw new ApiError(422,"ATTACHMENT_SOURCE_REQUIRED","Choose a quarantined file source.");
  await drainAttachmentScans(db,files,attachmentId);
  const row=await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(attachmentId).first<{source_state:AttachmentSourceState}>();
  if(!row)throw new ApiError(404,"ATTACHMENT_SOURCE_NOT_FOUND","File source not found.");
  return row.source_state;
}

export async function attachmentDestructionEligibility(db:SqlDatabase,attachmentId:string,now=Date.now()){
  const source=await db.prepare("SELECT source_state,legal_hold FROM attachments WHERE id=?").bind(attachmentId).first<{source_state:string;legal_hold:number}>();
  if(!source)return {eligible:false,reason:"source_missing",retainUntil:null};
  if(Number(source.legal_hold)!==0)return {eligible:false,reason:"legal_hold",retainUntil:null};
  if(await activeAttachmentLegalPreservation(db,attachmentId))return {eligible:false,reason:"legal_preservation",retainUntil:null};
  if(source.source_state!=="available")return {eligible:false,reason:"source_not_available",retainUntil:null};
  const obligations=await db.prepare(`SELECT
      count(*) FILTER(WHERE l.state IN ('draft','bound')) active_links,
      count(*) FILTER(WHERE l.state IN ('expired','withdrawn') AND l.expires_at<=?) closed_due_links,
      count(*) FILTER(WHERE s.retention_policy_key IS NOT NULL AND (s.destruction_mode IS DISTINCT FROM 'automatic' OR s.retain_until IS NULL)) review_obligations,
      max(s.retain_until) FILTER(WHERE s.retention_policy_key IS NOT NULL) retain_until
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.attachment_id=?`).bind(now,attachmentId).first<{active_links:number;closed_due_links:number;review_obligations:number;retain_until:number|null}>();
  if(Number(obligations?.active_links??0)>0)return {eligible:false,reason:"active_link",retainUntil:obligations?.retain_until??null};
  if(Number(obligations?.closed_due_links??0)<1)return {eligible:false,reason:"draft_retention",retainUntil:obligations?.retain_until??null};
  if(Number(obligations?.review_obligations??0)>0)return {eligible:false,reason:"retention_review",retainUntil:obligations?.retain_until??null};
  const retainUntil=obligations?.retain_until===null||obligations?.retain_until===undefined?null:Number(obligations.retain_until);
  if(retainUntil!==null&&retainUntil>now)return {eligible:false,reason:"retain_until",retainUntil};
  return {eligible:true,reason:"eligible",retainUntil};
}

async function activeAttachmentLegalPreservation(db:SqlDatabase,attachmentId:string){
  return Boolean(await db.prepare(`SELECT 1 FROM (
    SELECT p.item_id FROM legal_file_preservations p
    JOIN legal_hold_targets target ON target.item_id=p.item_id
    JOIN legal_holds hold ON hold.id=target.hold_id
    WHERE p.source_type='attachment' AND p.source_id=? AND hold.state<>'released'
    UNION ALL
    SELECT set_row.target_id FROM attachment_links link
    JOIN attachment_sets set_row ON set_row.id=link.set_id
    JOIN legal_hold_targets target ON target.item_id=set_row.target_id
    JOIN legal_holds hold ON hold.id=target.hold_id
    WHERE link.attachment_id=? AND set_row.target_type='legal_item' AND hold.state<>'released'
  ) active LIMIT 1`).bind(attachmentId,attachmentId).first());
}

export async function cleanupAttachmentDrafts(db:SqlDatabase,files:FileStore){
  if(!attachmentsV2Enabled()||attachmentUploadsFrozen())return {expired:0,expiredSets:0,destroyed:0,bytesDeleted:0,deletionFailures:0};const now=Date.now();
  // Leave approval uses the owner row as the manifest serialization boundary.
  // Exclude Leave from bulk expiry and process each request while holding that
  // same row lock, so cleanup cannot invalidate a manifest between its guards
  // and bind steps.
  const leaveTargets=(await db.prepare(`SELECT DISTINCT set_row.company_id,set_row.target_id
    FROM attachment_sets set_row
    WHERE set_row.target_type='leave_request' AND set_row.state='draft' AND (
      EXISTS(SELECT 1 FROM attachment_links link WHERE link.set_id=set_row.id AND link.state='draft' AND link.expires_at<=?)
      OR (NOT EXISTS(SELECT 1 FROM attachment_links link WHERE link.set_id=set_row.id) AND set_row.created_at<=?)
    ) ORDER BY set_row.company_id,set_row.target_id`).bind(now,now-ATTACHMENT_DRAFT_MS).all<{company_id:string;target_id:string}>()).results;
  const expired=(await db.prepare(`UPDATE attachment_links link SET state='expired' FROM attachment_sets set_row
    WHERE link.set_id=set_row.id AND set_row.target_type<>'leave_request' AND link.state='draft' AND link.expires_at<=?
    RETURNING link.id,link.set_id,link.attachment_id`).bind(now).all<{id:string;set_id:string;attachment_id:string|null}>()).results;
  for(const link of expired)await db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) SELECT ?,company_id,?,?,?,?,?,?,? FROM attachment_sets WHERE id=?")
    .bind(`attachment-event-${randomUUID()}`,link.attachment_id,link.id,link.set_id,"system:jobs","link.expired","{}",now,link.set_id).run();
  await db.prepare(`UPDATE upload_intents intent SET status='expired',updated_at=? WHERE intent.attachment_set_id IS NOT NULL AND intent.status='pending' AND intent.expires_at<=?
    AND NOT EXISTS(SELECT 1 FROM attachment_sets set_row WHERE set_row.id=intent.attachment_set_id AND set_row.target_type='leave_request')`).bind(now,now).run();
  const expiredSets=(await db.prepare(`UPDATE attachment_sets s SET state='expired',updated_at=? WHERE s.state='draft' AND s.target_type<>'leave_request'
    AND NOT EXISTS(SELECT 1 FROM attachment_links active WHERE active.set_id=s.id AND active.state IN ('draft','bound'))
    AND ((EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM attachment_links future_link WHERE future_link.set_id=s.id AND future_link.expires_at>?))
      OR (NOT EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=s.id) AND s.created_at<=?))
    RETURNING id,company_id`).bind(now,now,now-ATTACHMENT_DRAFT_MS).all<{id:string;company_id:string}>()).results;
  for(const set of expiredSets)await db.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,'system:jobs','set.expired','{}',?)")
    .bind(`attachment-event-${randomUUID()}`,set.company_id,set.id,now).run();
  for(const target of leaveTargets){
    const result=await atomic(db,async tx=>{
      const owner=await tx.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE").bind(target.target_id,target.company_id).first<{id:string}>();
      if(!owner)return {links:[] as {id:string;set_id:string;attachment_id:string|null}[],sets:[] as {id:string;company_id:string}[]};
      const links=(await tx.prepare(`UPDATE attachment_links link SET state='expired' FROM attachment_sets set_row
        WHERE link.set_id=set_row.id AND set_row.company_id=? AND set_row.target_type='leave_request' AND set_row.target_id=?
          AND set_row.state='draft' AND link.state='draft' AND link.expires_at<=?
        RETURNING link.id,link.set_id,link.attachment_id`).bind(target.company_id,target.target_id,now).all<{id:string;set_id:string;attachment_id:string|null}>()).results;
      for(const link of links)await tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,'system:jobs','link.expired','{}',?)")
        .bind(`attachment-event-${randomUUID()}`,target.company_id,link.attachment_id,link.id,link.set_id,now).run();
      await tx.prepare(`UPDATE upload_intents intent SET status='expired',updated_at=? FROM attachment_sets set_row
        WHERE intent.attachment_set_id=set_row.id AND set_row.company_id=? AND set_row.target_type='leave_request' AND set_row.target_id=?
          AND set_row.state='draft' AND intent.status='pending' AND intent.expires_at<=?`).bind(now,target.company_id,target.target_id,now).run();
      const sets=(await tx.prepare(`UPDATE attachment_sets set_row SET state='expired',updated_at=?
        WHERE set_row.company_id=? AND set_row.target_type='leave_request' AND set_row.target_id=? AND set_row.state='draft'
          AND NOT EXISTS(SELECT 1 FROM attachment_links active WHERE active.set_id=set_row.id AND active.state IN ('draft','bound'))
          AND ((EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=set_row.id)
            AND NOT EXISTS(SELECT 1 FROM attachment_links future_link WHERE future_link.set_id=set_row.id AND future_link.expires_at>?))
            OR (NOT EXISTS(SELECT 1 FROM attachment_links any_link WHERE any_link.set_id=set_row.id) AND set_row.created_at<=?))
        RETURNING set_row.id,set_row.company_id`).bind(now,target.company_id,target.target_id,now,now-ATTACHMENT_DRAFT_MS).all<{id:string;company_id:string}>()).results;
      for(const set of sets)await tx.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,'system:jobs','set.expired','{}',?)")
        .bind(`attachment-event-${randomUUID()}`,set.company_id,set.id,now).run();
      return {links,sets};
    });
    expired.push(...result.links);expiredSets.push(...result.sets);
  }
  const sources=(await db.prepare(`SELECT a.id FROM attachments a WHERE a.company_id IS NOT NULL AND a.legal_hold=0 AND a.source_state='available'
    AND EXISTS(SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN ('expired','withdrawn') AND closed.expires_at<=?)
    AND NOT EXISTS(SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN ('draft','bound'))
    AND NOT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.attachment_id=a.id AND s.retention_policy_key IS NOT NULL
      AND (s.destruction_mode IS DISTINCT FROM 'automatic' OR s.retain_until IS NULL OR s.retain_until>?))
    AND NOT EXISTS(SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets target ON target.item_id=p.item_id JOIN legal_holds hold ON hold.id=target.hold_id
      WHERE p.source_type='attachment' AND p.source_id=a.id AND hold.state<>'released')
    AND NOT EXISTS(SELECT 1 FROM attachment_links legal_link JOIN attachment_sets legal_set ON legal_set.id=legal_link.set_id
      JOIN legal_hold_targets target ON target.item_id=legal_set.target_id JOIN legal_holds hold ON hold.id=target.hold_id
      WHERE legal_link.attachment_id=a.id AND legal_set.target_type='legal_item' AND hold.state<>'released') LIMIT 100`).bind(now,now).all<{id:string}>()).results;
  let destroyed=0;
  for(const source of sources){
    const sealed=await atomic(db,async tx=>{
      await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-destroy:${source.id}`).run();
      await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();
      const locked=await tx.prepare("SELECT id FROM attachments WHERE id=? AND source_state='available' FOR UPDATE").bind(source.id).first<{id:string}>();
      if(!locked||!(await attachmentDestructionEligibility(tx,source.id,now)).eligible)return false;
      const transitioned=await tx.prepare("UPDATE attachments SET source_state='destroyed',destroyed_at=? WHERE id=? AND source_state='available' AND legal_hold=0 RETURNING company_id")
        .bind(now,source.id).first<{company_id:string}>();
      if(!transitioned)return false;
      await tx.prepare("UPDATE attachment_storage_ledger SET state='destroyed',destroyed_at=? WHERE attachment_id=? AND state<>'destroyed'").bind(now,source.id).run();
      await tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,'system:jobs','source.destroyed','{}',?)")
        .bind(`attachment-event-${randomUUID()}`,transitioned.company_id,source.id,now).run();
      return true;
    });
    if(sealed)destroyed++;
  }
  const pendingDeletion=(await db.prepare(`SELECT a.id,a.object_key,a.preview_object_key FROM attachments a
    WHERE a.source_state='destroyed' AND a.legal_hold=0
      AND NOT EXISTS(SELECT 1 FROM attachment_events e WHERE e.attachment_id=a.id AND e.action='source.bytes_deleted')
    ORDER BY a.destroyed_at,a.id LIMIT 100`).all<{id:string;object_key:string;preview_object_key:string|null}>()).results;
  let bytesDeleted=0,deletionFailures=0;
  for(const source of pendingDeletion){
    try{
      const deleted=await atomic(db,async tx=>{
        await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`attachment-destroy:${source.id}`).run();
        await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();
        const locked=await tx.prepare("SELECT company_id,object_key,preview_object_key,source_state,legal_hold FROM attachments WHERE id=? FOR UPDATE")
          .bind(source.id).first<{company_id:string;object_key:string;preview_object_key:string|null;source_state:string;legal_hold:number}>();
        if(!locked||locked.source_state!=="destroyed"||Number(locked.legal_hold)!==0||await activeAttachmentLegalPreservation(tx,source.id))return false;
        if(await tx.prepare("SELECT 1 FROM attachment_events WHERE attachment_id=? AND action='source.bytes_deleted'").bind(source.id).first())return false;
        const ledgerKeys=(await tx.prepare("SELECT object_key FROM attachment_storage_ledger WHERE attachment_id=?").bind(source.id).all<{object_key:string}>()).results.map(row=>row.object_key);
        await files.delete([...new Set([locked.object_key,...(locked.preview_object_key?[locked.preview_object_key]:[]),...ledgerKeys])],files.supportsTransactionDeletion?{database:tx}:undefined);
        const eventId=`attachment-bytes-deleted-${createHash("sha256").update(source.id).digest("hex").slice(0,32)}`;
        await tx.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,'system:jobs','source.bytes_deleted','{}',?) ON CONFLICT(id) DO NOTHING")
          .bind(eventId,locked.company_id,source.id,Date.now()).run();
        return true;
      });
      if(deleted)bytesDeleted++;
    }catch(error){
      deletionFailures++;
      const code=(error instanceof Error?error.message:"ATTACHMENT_BYTE_DELETE_FAILED").slice(0,160);
      await db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) SELECT ?,company_id,?,'system:jobs','source.byte_delete_failed',?,? FROM attachments WHERE id=?")
        .bind(`attachment-event-${randomUUID()}`,source.id,safeJson({code}),Date.now(),source.id).run();
    }
  }
  return {expired:expired.length,expiredSets:expiredSets.length,destroyed,bytesDeleted,deletionFailures};
}
