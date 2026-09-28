import "server-only";

import { hasModuleAccess } from "@/data/module-access";
import { samePerson } from "@/lib/iam";
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
  options: Partial<
    Omit<AttachmentPolicy, "targetType" | "purposeKey" | "componentKinds">
  > = {},
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
    classification: options.classification ?? "internal",
    retentionPolicyKey: options.retentionPolicyKey ?? null,
    publication: options.publication ?? false,
    portalUpload: false,
  };
}

/**
 * Closed additions for real operational owners that are not projections.
 * Existing owner types in this list continue to use their established target
 * authorization; only the five new owner types below are resolved here.
 */
export const remainingOperationalAttachmentPolicies: readonly AttachmentPolicy[] =
  Object.freeze([
    policy("purchase_order", "supplier_promise", ["action"], {
      classification: "confidential",
    }),
    policy("supplier_bill_match", "invoice_document", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("supplier_bill_match", "tax_document", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("supplier_bill_match", "bill_match_support", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("commerce_order", "signed_quote", ["record"], {
      classification: "confidential",
    }),
    policy("inventory_transfer", "direct_receipt_evidence", ["action"], {
      classification: "confidential",
    }),
    policy("inventory_quality_spec", "controlled_specification", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("asset", "acceptance", ["record"], { maxFiles: 50 }),
    policy("asset", "handover", ["record"], { maxFiles: 50 }),
    policy("asset", "data_wipe", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("maintenance_work", "checklist", ["record"]),
    policy("maintenance_work", "agreement", ["record"], {
      classification: "confidential",
    }),
    policy("maintenance_work", "sla", ["record"], {
      classification: "confidential",
    }),
    policy("maintenance_work", "warranty", ["record"]),
    policy("ecommerce_listing", "listing_media", ["record"], {
      mediaTypes: images,
      maxFiles: 50,
      publication: true,
    }),
    policy("ecommerce_listing", "channel_document", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("ecommerce_listing", "compliance_document", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("marketing_asset", "creative", ["record"], {
      maxFiles: 50,
      publication: true,
    }),
    policy("marketing_asset", "source", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("marketing_asset", "consent", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("manufacturing_spec", "drawing", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("manufacturing_spec", "sop", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("manufacturing_spec", "formula", ["record"], {
      classification: "restricted",
      maxFiles: 50,
    }),
    policy("manufacturing_spec", "controlled_specification", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("production_run", "run_report_evidence", ["action"], {
      classification: "confidential",
    }),
    policy("production_run", "qc_evidence", ["action"], {
      classification: "confidential",
    }),
    policy("production_run", "rework_action_evidence", ["action"], {
      classification: "confidential",
    }),
    policy("planning_scenario", "assumption_source", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
    policy("planning_scenario", "capacity_source", ["record"], {
      classification: "confidential",
      maxFiles: 50,
    }),
  ]);

const ownedTargets = new Set([
  "supplier_bill_match",
  "inventory_quality_spec",
  "ecommerce_listing",
  "marketing_asset",
  "manufacturing_spec",
]);

const moduleScopedTargets = {
  asset: { table: "operational_assets", module: "assets", branch: "branch_id", department: "department_id" },
  maintenance_work: { table: "maintenance_work_orders", module: "maintenance", branch: "branch_id", department: "department_id" },
  production_run: { table: "manufacturing_orders", module: "manufacturing", branch: "branch_id", department: null },
  planning_scenario: { table: "planning_scenarios", module: "planning", branch: "branch_id", department: null },
} as const;

type TargetRow = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
};

function targetScope(row: TargetRow, operation: AttachmentOperation): AttachmentTargetScope {
  return {
    companyId: row.company_id,
    branchId: row.branch_id,
    departmentId: row.department_id,
    version: Number(row.version),
    editable: operation !== "read",
  };
}

function requireScope(actor: WorkContext, row: TargetRow) {
  if (actor.branchId && row.branch_id && actor.branchId !== row.branch_id)
    throw new ApiError(
      403,
      "ATTACHMENT_SCOPE_FORBIDDEN",
      "The file owner is outside your Branch scope.",
    );
  if (
    actor.departmentId &&
    row.department_id &&
    actor.departmentId !== row.department_id
  )
    throw new ApiError(
      403,
      "ATTACHMENT_SCOPE_FORBIDDEN",
      "The file owner is outside your Department scope.",
    );
}

function requireExactScope(actor: WorkContext, row: TargetRow) {
  if (actor.branchId && actor.branchId !== row.branch_id)
    throw new ApiError(
      403,
      "ATTACHMENT_SCOPE_FORBIDDEN",
      "The file owner is outside your Branch scope.",
    );
  if (actor.departmentId && actor.departmentId !== row.department_id)
    throw new ApiError(
      403,
      "ATTACHMENT_SCOPE_FORBIDDEN",
      "The file owner is outside your Department scope.",
    );
}

export async function authorizeRemainingOperationalAttachmentTarget(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope | null> {
  if (
    policy.targetType === "inventory_transfer" &&
    policy.purposeKey === "direct_receipt_evidence"
  ) {
    const row = await db
      .prepare(
        "SELECT company_id,source_branch_id,target_branch_id,NULL::text department_id,version,status,created_by,prepared_by,delivery_mode FROM inventory_transfers WHERE id=? AND company_id=?",
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow & {
        source_branch_id: string;
        target_branch_id: string;
        status: string;
        created_by: string;
        prepared_by: string;
        delivery_mode: string;
      }>();
    if (!row)
      throw new ApiError(404, "ATTACHMENT_TARGET_NOT_FOUND", "The warehouse transfer was not found in this Company.");
    if (row.delivery_mode !== "direct")
      throw new ApiError(422, "ATTACHMENT_PURPOSE_OWNER", "Direct receipt evidence belongs only to a direct warehouse transfer.");
    if (operation === "read") {
      if (!hasPermission(actor, "inventory.read") || actor.departmentId)
        throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Inventory read authority is required for transfer evidence.");
      if (actor.branchId && ![row.source_branch_id, row.target_branch_id].includes(actor.branchId))
        throw new ApiError(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The transfer is outside your Branch scope.");
    } else {
      if (
        actor.departmentId ||
        !hasPermission(actor, "inventory.transfer.review") ||
        row.status !== "submitted" ||
        samePerson(actor, row.created_by) ||
        samePerson(actor, row.prepared_by) ||
        (actor.branchId &&
          (!hasPermission(actor, "inventory.prepare") ||
            actor.branchId !== row.target_branch_id))
      )
        throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "The current independent receiving reviewer is required for direct transfer evidence.");
    }
    return {
      companyId: row.company_id,
      branchId: row.target_branch_id,
      departmentId: null,
      version: Number(row.version),
      editable: operation !== "read",
    };
  }
  const moduleTarget = moduleScopedTargets[policy.targetType as keyof typeof moduleScopedTargets];
  if (moduleTarget) {
    if (
      !hasPermission(actor, operation === "read" ? "records.read" : "records.write") ||
      !hasModuleAccess(actor.modules, moduleTarget.module)
    )
      throw new ApiError(
        403,
        "ATTACHMENT_FORBIDDEN",
        `Your current assignment cannot access ${moduleTarget.module} files.`,
      );
    const row = await db
      .prepare(
        `SELECT company_id,${moduleTarget.branch} branch_id,${moduleTarget.department ?? "NULL::text"} department_id,version FROM ${moduleTarget.table} WHERE id=? AND company_id=?`,
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow>();
    if (!row)
      throw new ApiError(
        404,
        "ATTACHMENT_TARGET_NOT_FOUND",
        "The file owner was not found in this Company.",
      );
    if (actor.branchId && row.branch_id !== actor.branchId)
      throw new ApiError(
        403,
        "ATTACHMENT_SCOPE_FORBIDDEN",
        "The file owner is outside your Branch scope.",
      );
    if (actor.departmentId && row.department_id !== actor.departmentId)
      throw new ApiError(
        403,
        "ATTACHMENT_SCOPE_FORBIDDEN",
        "The file owner is outside your Department scope.",
      );
    return targetScope(row, operation);
  }
  if (
    policy.targetType === "purchase_order" &&
    policy.purposeKey === "supplier_promise"
  ) {
    if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.purchase"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Procurement authority is required for supplier promise evidence.");
    const row = await db
      .prepare("SELECT company_id,branch_id,department_id,version,state FROM commerce_orders WHERE id=? AND company_id=? AND kind='purchase'")
      .bind(targetId, actor.companyId)
      .first<TargetRow & { state: string }>();
    if (!row)
      throw new ApiError(404, "ATTACHMENT_TARGET_NOT_FOUND", "The purchase order was not found in this Company.");
    requireExactScope(actor, row);
    if (operation !== "read" && !["confirmed", "partially_fulfilled"].includes(row.state))
      throw new ApiError(409, "ATTACHMENT_TARGET_STATE", "Supplier promise evidence requires an approved outstanding purchase order.");
    return targetScope(row, operation);
  }
  if (
    policy.targetType === "commerce_order" &&
    policy.purposeKey === "signed_quote"
  ) {
    if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Sales authority is required for signed quotation documents.");
    const row = await db
      .prepare("SELECT company_id,branch_id,department_id,version FROM commerce_orders WHERE id=? AND company_id=? AND kind IN ('quote','sales')")
      .bind(targetId, actor.companyId)
      .first<TargetRow>();
    if (!row)
      throw new ApiError(404, "ATTACHMENT_TARGET_NOT_FOUND", "The quotation or sales order was not found in this Company.");
    requireExactScope(actor, row);
    return targetScope(row, operation);
  }
  if (!ownedTargets.has(policy.targetType)) return null;

  let row: TargetRow | null = null;
  if (policy.targetType === "supplier_bill_match") {
    if (!hasPermission(actor, operation === "read" ? "finance.read" : "finance.write"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Finance authority is required for supplier invoice documents.");
    row = await db
      .prepare(
        "SELECT company_id,branch_id,department_id,version FROM procurement_bill_matches WHERE id=? AND company_id=?",
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow>();
  } else if (policy.targetType === "inventory_quality_spec") {
    if (!hasPermission(actor, operation === "read" ? "quality.read" : "quality.release"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Quality authority is required for controlled specification documents.");
    if (operation !== "read" && (actor.branchId || actor.departmentId))
      throw new ApiError(403, "ATTACHMENT_SCOPE_FORBIDDEN", "Use a Company Quality assignment to manage controlled specifications.");
    row = await db
      .prepare(
        "SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM inventory_quality_specs WHERE id=? AND company_id=?",
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow>();
  } else if (policy.targetType === "ecommerce_listing") {
    if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales") || !hasModuleAccess(actor.modules, "ecommerce"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "E-commerce authority is required for listing files.");
    row = await db
      .prepare(
        "SELECT c.company_id,NULL::text branch_id,NULL::text department_id,l.version FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow>();
  } else if (policy.targetType === "marketing_asset") {
    if (!hasPermission(actor, operation === "read" ? "records.read" : "records.write") || !hasModuleAccess(actor.modules, "marketing"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Marketing authority is required for asset files.");
    row = await db
      .prepare(
        "SELECT company_id,NULL::text branch_id,NULL::text department_id,version,kind FROM marketing_assets WHERE id=? AND company_id=?",
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow & { kind: string }>();
    if (
      row &&
      ["creative", "consent"].includes(policy.purposeKey) &&
      (row as TargetRow & { kind: string }).kind !== "content"
    )
      throw new ApiError(
        422,
        "ATTACHMENT_PURPOSE_OWNER",
        "Creative media and rights evidence belong to a content marketing asset.",
      );
  } else if (policy.targetType === "manufacturing_spec") {
    if (!hasPermission(actor, operation === "read" ? "records.read" : "records.write") || !hasModuleAccess(actor.modules, "manufacturing"))
      throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Manufacturing authority is required for controlled specification files.");
    row = await db
      .prepare(
        "SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM manufacturing_specifications WHERE id=? AND company_id=?",
      )
      .bind(targetId, actor.companyId)
      .first<TargetRow>();
  }

  if (!row)
    throw new ApiError(
      404,
      "ATTACHMENT_TARGET_NOT_FOUND",
      "The file owner was not found in this Company.",
    );
  if (policy.targetType === "supplier_bill_match") requireExactScope(actor, row);
  else requireScope(actor, row);
  return targetScope(row, operation);
}

const versionOwners: Record<string, string> = {
  supplier_bill_match: "procurement_bill_matches",
  inventory_quality_spec: "inventory_quality_specs",
  ecommerce_listing: "ecommerce_listings",
  marketing_asset: "marketing_assets",
  manufacturing_spec: "manufacturing_specifications",
};

export function remainingOperationalAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
  if (targetType === "ecommerce_listing")
    return assertion(
      db,
      commandId,
      `attachment-owner-${index}`,
      "EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=? AND l.version=?)",
      [targetId, companyId, version],
    );
  const table = versionOwners[targetType];
  if (!table) return null;
  return assertion(
    db,
    commandId,
    `attachment-owner-${index}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
    [targetId, companyId, version],
  );
}

export function remainingOperationalAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
): SqlStatement | null {
  const table = versionOwners[targetType];
  if (!table) return null;
  return db
    .prepare(
      `UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?`,
    )
    .bind(now, targetId, expectedVersion);
}
