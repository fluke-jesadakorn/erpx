import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import { assertion } from "../commands";
import { ApiError } from "../http";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import type {
  AttachmentOperation,
  AttachmentPolicy,
  AttachmentTargetScope,
} from "./types";

const MiB = 1024 * 1024;
const documents = [
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/heic",
  "image/heif",
  "application/pdf",
] as const;
const images = [
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/heic",
  "image/heif",
] as const;

function policy(
  targetType: string,
  purposeKey: string,
  componentKinds: AttachmentPolicy["componentKinds"],
  options: Partial<Omit<AttachmentPolicy, "targetType" | "purposeKey" | "componentKinds">> = {},
): AttachmentPolicy {
  return {
    targetType,
    purposeKey,
    componentKinds,
    version: 1,
    mediaTypes: options.mediaTypes ?? documents,
    maxFiles: options.maxFiles ?? 10,
    maxBytes: options.maxBytes ?? 10 * MiB,
    requiredCount: options.requiredCount ?? 0,
    classification: options.classification ?? "confidential",
    retentionPolicyKey: options.retentionPolicyKey ?? null,
    publication: false,
    portalUpload: false,
  };
}

/**
 * Closed inventory for Finance, POS, hospitality, working-time, Organization,
 * IAM and release/provisioning records that have a live table and workflow.
 * Deliberately absent: ledgers, reports, Today/Executive projections, secrets,
 * configuration dumps and database dumps.
 */
export const financeOperationsAttachmentPolicies: readonly AttachmentPolicy[] = Object.freeze([
  policy("finance_journal", "journal_support", ["record"], { maxFiles: 50 }),
  policy("finance_bank_statement", "bank_statement", ["record"], { maxFiles: 50 }),
  policy("finance_bank_transaction", "reconciliation_workpaper", ["action"]),
  policy("finance_period", "close_workpaper", ["action"], { maxFiles: 50 }),
  policy("finance_accrual_schedule", "accrual_support", ["record"], { maxFiles: 50 }),
  policy("finance_asset", "asset_support", ["record"], { maxFiles: 50 }),
  policy("finance_lease", "lease_support", ["record"], { maxFiles: 50 }),
  policy("finance_loan", "loan_support", ["record"], { maxFiles: 50 }),
  policy("finance_revenue_contract", "revenue_support", ["record"], { maxFiles: 50 }),
  policy("finance_payment", "payment_proof", ["action"]),
  policy("finance_refund_obligation", "refund_support", ["record"]),
  policy("finance_refund_obligation", "refund_proof", ["action"]),
  policy("finance_supplier_batch", "payment_batch_support", ["record"]),
  policy("finance_supplier_batch", "payment_proof", ["action"]),

  policy("pos_sale", "void_evidence", ["action"]),
  policy("pos_refund", "refund_evidence", ["action"]),
  policy("pos_shift", "cash_variance", ["action"]),

  policy("pms_guest", "identity_document", ["record"], { classification: "restricted" }),
  policy("pms_guest", "consent", ["record"], { classification: "restricted" }),
  policy("pms_reservation", "voucher", ["record"]),
  policy("pms_stay", "signed_registration", ["record"], { classification: "restricted" }),
  policy("pms_stay", "transfer_slip", ["record"], { classification: "restricted" }),
  policy("pms_task", "room_photo", ["action"], { mediaTypes: images }),
  policy("pms_task", "damage_photo", ["action"], { mediaTypes: images }),
  policy("pms_lost_property", "lost_property_photo", ["record", "action"], { mediaTypes: images }),
  policy("pms_lost_property", "handover_evidence", ["action"], { classification: "restricted" }),
  policy("pms_property", "ota_reconciliation", ["action"], { maxFiles: 50 }),

  policy("overtime_request", "planned_evidence", ["action"]),
  policy("overtime_request", "actual_evidence", ["action"]),
  policy("overtime_request", "exception_evidence", ["action"]),
  policy("time_entry", "corrected_entry_proof", ["action"]),

  policy("organization_node", "registration", ["record"], { maxFiles: 50 }),
  policy("organization_node", "license", ["record"], { maxFiles: 50 }),
  policy("organization_node", "branch_permit", ["record"], { maxFiles: 50 }),
  policy("organization_change", "org_change_proof", ["action"], { classification: "restricted" }),

  policy("iam_review", "sanitized_review_evidence", ["record", "action"], { classification: "restricted" }),
  policy("iam_migration", "migration_evidence", ["record", "action"], { classification: "restricted" }),
  policy("company_release_acceptance", "sanitized_release_evidence", ["record", "action"], { classification: "restricted" }),
  policy("people_provision_operation", "sanitized_provisioning_evidence", ["record", "action"], { classification: "restricted" }),
  policy("company_provisioning", "sanitized_provisioning_evidence", ["record"], { classification: "restricted" }),
]);

const targetTypes = new Set(financeOperationsAttachmentPolicies.map((item) => item.targetType));

type Row = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
  kind?: string;
  created_by?: string;
  requested_by?: string;
  submitted_by?: string;
  state?: string;
  status?: string;
};

function forbidden(message = "Your current assignment cannot access files for this record."): never {
  throw new ApiError(403, "ATTACHMENT_FORBIDDEN", message);
}

function assertWorkspaceScope(actor: WorkContext, row: Row) {
  if (actor.branchId && row.branch_id && actor.branchId !== row.branch_id)
    throw new ApiError(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
  if (actor.departmentId && row.department_id && actor.departmentId !== row.department_id)
    throw new ApiError(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
}

function result(row: Row, operation: AttachmentOperation): AttachmentTargetScope {
  return {
    companyId: row.company_id,
    branchId: row.branch_id,
    departmentId: row.department_id,
    version: Number(row.version),
    editable: operation !== "read",
  };
}

async function companyRow(
  db: SqlDatabase,
  actor: WorkContext,
  sql: string,
  targetId: string,
): Promise<Row> {
  const row = await db.prepare(sql).bind(targetId, actor.companyId).first<Row>();
  if (!row) throw new ApiError(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this Company.");
  assertWorkspaceScope(actor, row);
  return row;
}

const financeRecords: Record<string, { table: string; version: string; write: string }> = {
  finance_journal: { table: "finance_journals", version: "version", write: "finance.write" },
  finance_bank_statement: { table: "finance_bank_statements", version: "version", write: "finance.write" },
  finance_bank_transaction: { table: "finance_bank_transactions", version: "version", write: "finance.write" },
  finance_period: { table: "finance_periods", version: "close_version", write: "finance.write" },
  finance_accrual_schedule: { table: "finance_accrual_schedules", version: "version", write: "finance.write" },
  finance_asset: { table: "finance_assets", version: "version", write: "finance.write" },
  finance_lease: { table: "finance_leases", version: "version", write: "finance.write" },
  finance_loan: { table: "finance_loans", version: "version", write: "finance.write" },
  finance_revenue_contract: { table: "finance_revenue_contracts", version: "version", write: "finance.write" },
  finance_payment: { table: "finance_payments", version: "version", write: "finance.pay" },
  finance_refund_obligation: { table: "finance_refund_obligations", version: "version", write: "finance.pay" },
  finance_supplier_batch: { table: "finance_supplier_batches", version: "version", write: "finance.pay" },
};

const pmsKinds: Record<string, string> = {
  pms_guest: "guest",
  pms_reservation: "reservation",
  pms_stay: "stay",
  pms_task: "task",
  pms_lost_property: "lost",
};

function pmsWritePermission(policy: AttachmentPolicy) {
  if (["pms_guest", "pms_reservation"].includes(policy.targetType)) return "pms.reservations";
  if (policy.targetType === "pms_stay") return "pms.frontdesk";
  if (policy.targetType === "pms_task") return "pms.housekeeping";
  if (policy.targetType === "pms_lost_property") return "pms.housekeeping";
  if (policy.targetType === "pms_property" && policy.purposeKey === "ota_reconciliation") return "pms.cashier";
  return "pms.audit";
}

export async function authorizeFinanceOperationsAttachmentTarget(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope | null> {
  if (!targetTypes.has(policy.targetType)) return null;

  const finance = financeRecords[policy.targetType];
  if (finance) {
    const permission = operation === "read"
      ? "finance.read"
      : policy.targetType === "finance_supplier_batch" && policy.purposeKey === "payment_batch_support"
        ? "finance.write"
        : finance.write;
    if (!hasPermission(actor, permission)) forbidden();
    if (operation !== "read" && (["finance_period", "finance_payment", "finance_supplier_batch"].includes(policy.targetType) || (policy.targetType === "finance_refund_obligation" && policy.purposeKey === "refund_proof")) && (actor.branchId || actor.departmentId))
      forbidden("Use a Company Finance assignment to manage this evidence.");
    const row = await companyRow(
      db,
      actor,
      `SELECT company_id,NULL::text branch_id,NULL::text department_id,${finance.version} AS version FROM ${finance.table} WHERE id=? AND company_id=?`,
      targetId,
    );
    if (policy.targetType === "finance_period" && Number(row.version) < 1)
      throw new ApiError(409, "ATTACHMENT_TARGET_NOT_READY", "Begin the period close before attaching close workpapers.");
    return result(row, operation);
  }

  if (["pos_sale", "pos_refund", "pos_shift"].includes(policy.targetType)) {
    const writePermission = policy.targetType === "pos_sale" ? "pos.sell" : "pos.approve";
    if (!hasPermission(actor, operation === "read" ? "pos.read" : writePermission)) forbidden();
    const table = policy.targetType === "pos_sale" ? "pos_sales" : policy.targetType === "pos_refund" ? "pos_refunds" : "pos_shifts";
    const row = policy.targetType === "pos_refund"
      ? await companyRow(db, actor, `SELECT company_id,branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`, targetId)
      : await companyRow(db, actor, `SELECT company_id,branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`, targetId);
    return result(row, operation);
  }

  if (policy.targetType in pmsKinds) {
    if (!hasPermission(actor, operation === "read" ? "pms.read" : pmsWritePermission(policy))) forbidden();
    const row = await companyRow(
      db,
      actor,
      `SELECT p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind
       FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
       WHERE r.id=? AND p.company_id=?`,
      targetId,
    );
    if (row.kind !== pmsKinds[policy.targetType])
      throw new ApiError(404, "ATTACHMENT_TARGET_NOT_FOUND", "The selected hotel record does not match this file owner type.");
    return result(row, operation);
  }

  if (policy.targetType === "pms_property") {
    if (!hasPermission(actor, operation === "read" ? "pms.read" : pmsWritePermission(policy))) forbidden();
    return result(await companyRow(db, actor, "SELECT company_id,branch_id,NULL::text department_id,version FROM pms_properties WHERE id=? AND company_id=?", targetId), operation);
  }

  if (policy.targetType === "overtime_request") {
    const detail = await (await import("../overtime")).getOvertime(actor, targetId, db);
    const own = detail.created_by === actor.userId;
    if (operation !== "read" && !own && !detail.approval.canDecide && !detail.approval.canOverride) forbidden("Only the requester or current eligible reviewer can manage overtime evidence.");
    return result({ company_id: detail.company_id, branch_id: detail.branch_id, department_id: detail.department_id, version: detail.version }, operation);
  }

  if (policy.targetType === "time_entry") {
    const row = await companyRow(db, actor, `SELECT t.company_id,e.branch_id,e.department_id,t.version,t.submitted_by,t.status
      FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=?`, targetId);
    const owner = row.submitted_by === actor.userId;
    if (operation === "read") {
      if (!owner && !hasPermission(actor, "people.time.manage")) forbidden();
    } else if (!owner || !["draft", "submitted", "rejected"].includes(String(row.status))) {
      forbidden("Only the employee who submitted an editable time entry can attach correction proof.");
    }
    return result(row, operation);
  }

  if (policy.targetType === "organization_node") {
    if (!hasPermission(actor, "organization.manage")) forbidden();
    if (operation !== "read" && actor.assignmentScope !== "holding") forbidden("Use Company Organization authority to manage official organization documents.");
    const row = await companyRow(db, actor, `SELECT company_id,CASE WHEN kind='branch' THEN id ELSE branch_id END branch_id,NULL::text department_id,hierarchy_revision AS version,kind
      FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('company','headquarters','branch','department')`, targetId);
    if (policy.purposeKey === "branch_permit" && !["branch", "headquarters"].includes(String(row.kind)))
      throw new ApiError(422, "ATTACHMENT_PURPOSE_OWNER", "A branch permit may be attached only to a Branch or Headquarters record.");
    return result(row, operation);
  }

  if (policy.targetType === "organization_change") {
    if (!hasPermission(actor, "organization.manage") || actor.assignmentScope !== "holding") forbidden("Use Company Organization authority to manage organization-change evidence.");
    return result(await companyRow(db, actor, "SELECT company_id,NULL::text branch_id,NULL::text department_id,version,requested_by,state FROM organization_node_changes WHERE id=? AND company_id=?", targetId), operation);
  }

  if (["iam_review", "iam_migration"].includes(policy.targetType)) {
    if (!hasPermission(actor, "iam.manage") || actor.assignmentScope !== "holding") forbidden("Company IAM authority is required for sanitized access evidence.");
    const table = policy.targetType === "iam_review" ? "iam_reviews" : "iam_migrations";
    return result(await companyRow(db, actor, `SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`, targetId), operation);
  }

  if (policy.targetType === "company_release_acceptance") {
    if (!hasPermission(actor, operation === "read" ? "records.read" : "organization.manage") || actor.assignmentScope !== "holding") forbidden("Company release evidence requires Company authority.");
    return result(await companyRow(db, actor, "SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM company_release_acceptances WHERE id=? AND company_id=?", targetId), operation);
  }

  if (policy.targetType === "people_provision_operation") {
    if (!hasPermission(actor, operation === "read" ? "iam.read" : "iam.directory.apply") || actor.assignmentScope !== "holding") forbidden("Company IT provisioning authority is required.");
    return result(await companyRow(db, actor, "SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM people_provision_operations WHERE id=? AND company_id=?", targetId), operation);
  }

  if (policy.targetType === "company_provisioning") {
    if (!hasPermission(actor, "organization.manage") || actor.assignmentScope !== "holding") forbidden("Company provisioning evidence requires Company authority.");
    return result(await companyRow(db, actor, "SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM company_provisioning WHERE company_id=? AND company_id=?", targetId), operation);
  }

  return null;
}

const versionDescriptors: Record<string, { table: string; column?: string; extra?: string }> = {
  finance_journal: { table: "finance_journals" },
  finance_bank_statement: { table: "finance_bank_statements" },
  finance_bank_transaction: { table: "finance_bank_transactions" },
  finance_period: { table: "finance_periods", column: "close_version" },
  finance_accrual_schedule: { table: "finance_accrual_schedules" },
  finance_asset: { table: "finance_assets" },
  finance_lease: { table: "finance_leases" },
  finance_loan: { table: "finance_loans" },
  finance_revenue_contract: { table: "finance_revenue_contracts" },
  finance_payment: { table: "finance_payments" },
  finance_refund_obligation: { table: "finance_refund_obligations" },
  finance_supplier_batch: { table: "finance_supplier_batches" },
  pos_sale: { table: "pos_sales" },
  pos_refund: { table: "pos_refunds" },
  pos_shift: { table: "pos_shifts" },
  pms_guest: { table: "pms_records", extra: "kind='guest'" },
  pms_reservation: { table: "pms_records", extra: "kind='reservation'" },
  pms_stay: { table: "pms_records", extra: "kind='stay'" },
  pms_task: { table: "pms_records", extra: "kind='task'" },
  pms_lost_property: { table: "pms_records", extra: "kind='lost'" },
  pms_property: { table: "pms_properties" },
  overtime_request: { table: "overtime_requests" },
  time_entry: { table: "hr_time_entries" },
  organization_change: { table: "organization_node_changes" },
  iam_review: { table: "iam_reviews" },
  iam_migration: { table: "iam_migrations" },
  company_release_acceptance: { table: "company_release_acceptances" },
  people_provision_operation: { table: "people_provision_operations" },
  company_provisioning: { table: "company_provisioning" },
};

export function financeOperationsAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
  if (targetType === "organization_node")
    return assertion(db, commandId, `attachment-owner-${index}`, "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND hierarchy_revision=?)", [targetId, companyId, version]);
  if (targetType === "company_provisioning")
    return assertion(db, commandId, `attachment-owner-${index}`, "EXISTS(SELECT 1 FROM company_provisioning WHERE company_id=? AND company_id=? AND version=?)", [targetId, companyId, version]);
  if (targetType in pmsKinds)
    return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=? AND r.version=? AND r.kind=?)`, [targetId, companyId, version, pmsKinds[targetType]]);
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""})`, [targetId, companyId, version]);
}

export function financeOperationsAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
): SqlStatement | null {
  if (targetType === "organization_node")
    return db.prepare("UPDATE organization_nodes SET hierarchy_revision=hierarchy_revision+1 WHERE id=? AND hierarchy_revision=?").bind(targetId, expectedVersion);
  if (targetType === "company_provisioning")
    return db.prepare("UPDATE company_provisioning SET version=version+1,updated_at=? WHERE company_id=? AND version=?").bind(now, targetId, expectedVersion);
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  const hasUpdatedAt = !["finance_bank_statements", "finance_periods", "finance_assets", "iam_reviews", "iam_migrations", "company_provisioning"].includes(item.table);
  return db.prepare(`UPDATE ${item.table} SET ${column}=${column}+1${hasUpdatedAt ? ",updated_at=?" : ""} WHERE id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""}`)
    .bind(...(hasUpdatedAt ? [now, targetId, expectedVersion] : [targetId, expectedVersion]));
}

export const forbiddenAttachmentPurposeKeys = Object.freeze([
  "password",
  "private_key",
  "secret",
  "config_dump",
  "database_dump",
]);

const sanitizedEvidenceTargets = new Set([
  "iam_review",
  "iam_migration",
  "company_release_acceptance",
  "people_provision_operation",
  "company_provisioning",
]);

export function requiresSanitizedEvidenceAttestation(targetType: string) {
  return sanitizedEvidenceTargets.has(targetType);
}

/** Filename/caption defense is intentionally stricter than MIME validation. */
export function assertSanitizedEvidenceMetadata(policy: AttachmentPolicy, filename: string, caption: string | null) {
  if (!requiresSanitizedEvidenceAttestation(policy.targetType)) return;
  const label = `${filename}\n${caption ?? ""}`.normalize("NFKC").toLocaleLowerCase();
  const unsafe = [
    /(^|[^a-z])(password|passwd|secret|credential)s?([^a-z]|$)/,
    /private[\s_.-]*key/,
    /(^|[/\\._-])id_(rsa|dsa|ecdsa|ed25519)([^a-z]|$)/,
    /(^|[/\\._-])?\.env(?:[^a-z0-9]|$)/,
    /(config|configuration)[\s_.-]*(dump|export|backup)/,
    /(database|db)[\s_.-]*(dump|export|backup)/,
    /\.(pem|key|p12|pfx|sql|dump|bak|sqlite|sqlite3|db)([^a-z0-9]|$)/,
  ].some((pattern) => pattern.test(label));
  if (unsafe)
    throw new ApiError(422, "ATTACHMENT_SENSITIVE_SYSTEM_MATERIAL", "Passwords, private keys, secrets, configuration exports and database dumps must never be uploaded. Attach only a sanitized review artifact.");
}
