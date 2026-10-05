import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import { assertion } from "../commands";
import { ApiError } from "../http";
import { canCompleteClearance, canReadClearanceEvidence } from "../retirement-clearance";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import { ATTACHMENT_MAX_BYTES } from "./feature";
import type {
  AttachmentOperation,
  AttachmentPolicy,
  AttachmentTargetScope,
} from "./types";

const documents = [
  "image/jpeg",
  "image/png",
  "image/webp",
  "image/heic",
  "image/heif",
  "application/pdf",
] as const;

type SensitivePolicySeed = Omit<
  AttachmentPolicy,
  | "version"
  | "componentKinds"
  | "mediaTypes"
  | "maxFiles"
  | "maxBytes"
  | "requiredCount"
  | "classification"
  | "retentionPolicyKey"
  | "publication"
  | "portalUpload"
> &
  Partial<
    Pick<
      AttachmentPolicy,
      | "componentKinds"
      | "mediaTypes"
      | "maxFiles"
      | "maxBytes"
      | "requiredCount"
      | "classification"
      | "retentionPolicyKey"
    >
  >;

const sensitiveSeeds: SensitivePolicySeed[] = [];
function add(
  targetType: string,
  purposeKeys: readonly string[],
  options: Omit<SensitivePolicySeed, "targetType" | "purposeKey"> = {},
) {
  for (const purposeKey of purposeKeys)
    sensitiveSeeds.push({ targetType, purposeKey, ...options });
}

add(
  "hr_person",
  ["identity_document", "work_permit", "tax_document", "bank_document"],
  { classification: "restricted" },
);
add(
  "hr_person",
  ["employment_contract", "employment_amendment", "professional_certificate"],
  { classification: "confidential" },
);
add("hr_person", ["compensation_authorization"], {
  classification: "restricted",
});
add("hr_evaluation_assessment", ["evaluation_evidence"], { classification: "restricted" });
add("people_lifecycle_case", ["offer_document", "transfer_document", "offboarding_document"], {
  classification: "restricted",
});
add("people_clearance_task", ["clearance_evidence"], {
  componentKinds: ["action"],
  classification: "restricted",
});
add("payroll_run", ["correction_support"], {
  componentKinds: ["action"],
  classification: "restricted",
});
add("payroll_advance", ["advance_support"], { classification: "restricted" });
add("payroll_final_pay", ["final_pay_support"], { classification: "restricted" });
add("talent_candidate", ["cv", "portfolio", "work_sample", "background_document"], {
  classification: "restricted",
  maxFiles: 50,
});
add("talent_offer", ["offer_document"], { classification: "restricted" });
add("learning_course", ["course_material"], {
  classification: "confidential",
  maxFiles: 50,
});
add("learning_assignment", ["certificate"], {
  componentKinds: ["record", "action"],
  classification: "confidential",
});
add("benefit_plan", ["benefit_booklet"], {
  classification: "confidential",
  maxFiles: 50,
});
add("benefit_dependent", ["dependent_proof"], {
  componentKinds: ["record", "action"],
  classification: "restricted",
});
add("benefit_election", ["election_document", "provider_proof"], {
  componentKinds: ["record", "action"],
  classification: "restricted",
});

/**
 * Closed policy inventory for sensitive workforce documents. Deliberately do
 * not register investigation or internal-HR-memo purposes: an
 * unknown purpose fails before any owner metadata can be returned.
 */
export const sensitiveAttachmentPolicies: readonly AttachmentPolicy[] = Object.freeze(
  sensitiveSeeds.map((seed) => ({
    targetType: seed.targetType,
    purposeKey: seed.purposeKey,
    version: 1,
    componentKinds: seed.componentKinds ?? (["record"] as const),
    mediaTypes: seed.mediaTypes ?? documents,
    maxFiles: seed.maxFiles ?? 10,
    maxBytes: seed.maxBytes ?? ATTACHMENT_MAX_BYTES,
    requiredCount: seed.requiredCount ?? 0,
    classification: seed.classification ?? "restricted",
    retentionPolicyKey: seed.retentionPolicyKey ?? null,
    publication: false,
    portalUpload: false,
  })),
);

const sensitiveTargetTypes = new Set(
  sensitiveAttachmentPolicies.map((policy) => policy.targetType),
);

export function isSensitiveAttachmentTarget(targetType: string) {
  return sensitiveTargetTypes.has(targetType);
}

function privateNotFound(): never {
  throw new ApiError(
    404,
    "ATTACHMENT_TARGET_NOT_FOUND",
    "The file owner was not found in this workspace.",
  );
}

function inScope(
  actor: WorkContext,
  row: { branch_id: string | null; department_id: string | null },
) {
  return (
    (!actor.branchId || actor.branchId === row.branch_id) &&
    (!actor.departmentId || actor.departmentId === row.department_id)
  );
}

function scope(
  row: {
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    version: number;
  },
  editable: boolean,
): AttachmentTargetScope {
  return {
    companyId: row.company_id,
    branchId: row.branch_id,
    departmentId: row.department_id,
    version: Number(row.version),
    editable,
  };
}

const employeeSelfVisible = new Set([
  "employment_contract",
  "employment_amendment",
  "professional_certificate",
]);

async function employeePerson(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT p.company_id,p.user_id,p.version,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,p.company_id) THEN 1 ELSE 0 END is_self
      FROM hr_people p
      LEFT JOIN LATERAL (
        SELECT x.branch_id,x.department_id FROM hr_engagements x
        WHERE x.person_id=p.id AND x.company_id=p.company_id
        ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id
        LIMIT 1
      ) e ON true
      WHERE p.id=? AND p.company_id=? AND p.merged_into_id IS NULL`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      user_id: string | null;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      is_self: number;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const self = Number(row.is_self) > 0;
  const read =
    hasPermission(actor, "people.documents.read") ||
    (self && employeeSelfVisible.has(policy.purposeKey));
  const write =
    hasPermission(actor, "people.documents.manage") ||
    (self && policy.purposeKey === "professional_certificate");
  if (operation === "read" ? !read : !write) privateNotFound();
  return scope(row, operation !== "read");
}

async function lifecycleCase(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT c.company_id,c.version,e.branch_id,e.department_id
      FROM people_lifecycle_cases c
      JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
      WHERE c.id=? AND c.company_id=?`,
    )
    .bind(targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
    }>();
  const permission = operation === "read" ? "people.documents.read" : "people.documents.manage";
  if (!row || !inScope(actor, row) || !hasPermission(actor, permission)) privateNotFound();
  return scope(row, operation !== "read");
}

async function clearanceTask(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT t.company_id,t.version,t.status,t.assignee_id,t.completed_by,t.clearance_type,
        e.branch_id,e.department_id
      FROM people_preparation_tasks t
      JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id
      JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
      WHERE t.id=? AND t.company_id=? AND t.clearance_type IS NOT NULL`,
    )
    .bind(targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      status: string;
      assignee_id: string | null;
      completed_by: string | null;
      clearance_type: import("@/lib/retirement").ClearanceKind;
      branch_id: string | null;
      department_id: string | null;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const allowed =
    operation === "read"
      ? canReadClearanceEvidence(actor, row)
      : row.status === "pending" && canCompleteClearance(actor, row);
  if (!allowed) privateNotFound();
  return scope(row, operation !== "read");
}

async function payrollRun(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      "SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM hr_payroll_runs WHERE id=? AND company_id=?",
    )
    .bind(targetId, actor.companyId)
    .first<{ company_id: string; branch_id: null; department_id: null; version: number }>();
  const permission = operation === "read" ? "payroll.documents.read" : "payroll.documents.manage";
  if (
    !row ||
    actor.branchId ||
    actor.departmentId ||
    !hasPermission(actor, permission)
  )
    privateNotFound();
  return scope(row, operation !== "read");
}

async function payrollAdvance(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT a.company_id,a.version,a.status,a.requested_by,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,a.company_id) THEN 1 ELSE 0 END is_self
      FROM hr_payroll_advances a
      JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id
      JOIN hr_people p ON p.id=a.person_id AND p.company_id=a.company_id
      WHERE a.id=? AND a.company_id=?`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      status: string;
      requested_by: string;
      branch_id: string | null;
      department_id: string | null;
      is_self: number;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const self = Number(row.is_self) > 0;
  const read = hasPermission(actor, "payroll.documents.read") || self;
  const write =
    hasPermission(actor, "payroll.documents.manage") ||
    (self && row.status === "pending");
  if (operation === "read" ? !read : !write) privateNotFound();
  return scope(row, operation !== "read");
}

async function finalPay(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT fp.company_id,fp.version,fp.status,t.assignee_id,t.completed_by,t.clearance_type,
        e.branch_id,e.department_id
      FROM hr_payroll_final_pay_preparations fp
      JOIN people_preparation_tasks t ON t.id=fp.task_id AND t.company_id=fp.company_id
      JOIN hr_engagements e ON e.id=fp.engagement_id AND e.company_id=fp.company_id
      WHERE fp.id=? AND fp.company_id=?`,
    )
    .bind(targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      status: string;
      assignee_id: string | null;
      completed_by: string | null;
      clearance_type: import("@/lib/retirement").ClearanceKind;
      branch_id: string | null;
      department_id: string | null;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const allowed =
    operation === "read"
      ? hasPermission(actor, "payroll.documents.read") &&
        canReadClearanceEvidence(actor, row)
      : hasPermission(actor, "payroll.documents.manage") &&
        row.status === "prepared" &&
        canCompleteClearance(actor, row);
  if (!allowed) privateNotFound();
  return scope(row, operation !== "read");
}

async function talentCandidate(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT c.company_id,c.version,
        min(r.branch_id) FILTER (WHERE a.owner_id=?) owner_branch_id,
        min(r.department_id) FILTER (WHERE a.owner_id=?) owner_department_id,
        CASE WHEN EXISTS(SELECT 1 FROM talent_applications owned WHERE owned.candidate_id=c.id AND owned.company_id=c.company_id AND owned.owner_id=?) THEN 1 ELSE 0 END is_owner
      FROM talent_candidates c
      LEFT JOIN talent_applications a ON a.candidate_id=c.id AND a.company_id=c.company_id
      LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE c.id=? AND c.company_id=?
      GROUP BY c.id,c.company_id,c.version`,
    )
    .bind(actor.userId, actor.userId, actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      owner_branch_id: string | null;
      owner_department_id: string | null;
      is_owner: number;
    }>();
  if (!row) privateNotFound();
  const scoped = {
    ...row,
    branch_id: row.owner_branch_id,
    department_id: row.owner_department_id,
  };
  const canManage = hasPermission(actor, "talent.documents.manage");
  const canRead =
    hasPermission(actor, "talent.documents.read") &&
    (canManage || Number(row.is_owner) > 0);
  if (!inScope(actor, scoped) || (operation === "read" ? !canRead : !canManage))
    privateNotFound();
  return scope(scoped, operation !== "read");
}

async function talentOffer(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT o.company_id,o.version,r.branch_id,r.department_id,a.owner_id
      FROM talent_offers o
      JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id
      JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE o.id=? AND o.company_id=?`,
    )
    .bind(targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      owner_id: string;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const canManage = hasPermission(actor, "talent.documents.manage");
  const canRead =
    hasPermission(actor, "talent.documents.read") &&
    (canManage || row.owner_id === actor.userId);
  if (operation === "read" ? !canRead : !canManage) privateNotFound();
  return scope(row, operation !== "read");
}

async function learningCourse(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT c.company_id,c.version,NULL::text branch_id,NULL::text department_id,c.status,
        CASE WHEN EXISTS(SELECT 1 FROM learning_assignments a WHERE a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled') THEN 1 ELSE 0 END is_learner
      FROM learning_courses c WHERE c.id=? AND c.company_id=?`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      branch_id: null;
      department_id: null;
      status: string;
      is_learner: number;
    }>();
  if (!row) privateNotFound();
  const read =
    hasPermission(actor, "learning.documents.read") ||
    (Number(row.is_learner) > 0 && ["published", "retired"].includes(row.status));
  const write = hasPermission(actor, "learning.documents.manage");
  if (operation === "read" ? !read : !write) privateNotFound();
  return scope(row, operation !== "read");
}

async function learningAssignment(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT a.company_id,a.version,a.branch_id,a.department_id,a.status,a.learner_id,
        CASE WHEN iam_same_person(a.learner_id,?,a.company_id) THEN 1 ELSE 0 END is_learner
      FROM learning_assignments a WHERE a.id=? AND a.company_id=?`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      status: string;
      learner_id: string;
      is_learner: number;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const self = Number(row.is_learner) > 0;
  const read = hasPermission(actor, "learning.documents.read") || self;
  const write =
    hasPermission(actor, "learning.documents.manage") ||
    (self && ["in_progress", "completed"].includes(row.status));
  if (operation === "read" ? !read : !write) privateNotFound();
  return scope(row, operation !== "read");
}

async function benefitPlan(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT p.company_id,p.version,NULL::text branch_id,NULL::text department_id,p.status,
        CASE WHEN EXISTS(SELECT 1 FROM hr_people h JOIN hr_engagements e ON e.person_id=h.id AND e.company_id=h.company_id WHERE h.company_id=p.company_id AND h.user_id=? AND h.status='active' AND h.merged_into_id IS NULL AND e.status='active') THEN 1 ELSE 0 END is_employee
      FROM benefit_plans p WHERE p.id=? AND p.company_id=?`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      branch_id: null;
      department_id: null;
      status: string;
      is_employee: number;
    }>();
  if (!row) privateNotFound();
  const read =
    hasPermission(actor, "benefits.documents.read") ||
    (Number(row.is_employee) > 0 && ["published", "retired"].includes(row.status));
  const write = hasPermission(actor, "benefits.documents.manage");
  if (operation === "read" ? !read : !write) privateNotFound();
  return scope(row, operation !== "read");
}

async function benefitDependent(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT d.company_id,d.version,d.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,d.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_dependents d
      JOIN hr_people p ON p.id=d.person_id AND p.company_id=d.company_id
      LEFT JOIN LATERAL (SELECT x.branch_id,x.department_id FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status='active' THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) e ON true
      WHERE d.id=? AND d.company_id=?`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      status: string;
      branch_id: string | null;
      department_id: string | null;
      is_owner: number;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const self = Number(row.is_owner) > 0;
  const read = hasPermission(actor, "benefits.documents.read") || self;
  const write =
    hasPermission(actor, "benefits.documents.manage") ||
    (self && row.status === "pending");
  if (operation === "read" ? !read : !write) privateNotFound();
  return scope(row, operation !== "read");
}

async function benefitElection(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
) {
  const row = await db
    .prepare(
      `SELECT x.company_id,x.version,x.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,x.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_elections x
      JOIN hr_people p ON p.id=x.person_id AND p.company_id=x.company_id
      JOIN hr_engagements e ON e.id=x.engagement_id AND e.company_id=x.company_id
      WHERE x.id=? AND x.company_id=?`,
    )
    .bind(actor.userId, targetId, actor.companyId)
    .first<{
      company_id: string;
      version: number;
      status: string;
      branch_id: string | null;
      department_id: string | null;
      is_owner: number;
    }>();
  if (!row || !inScope(actor, row)) privateNotFound();
  const self = Number(row.is_owner) > 0;
  const managerRead = hasPermission(actor, "benefits.documents.read");
  const managerWrite = hasPermission(actor, "benefits.documents.manage");
  const selfRead =
    self &&
    (policy.purposeKey === "election_document" ||
      (policy.purposeKey === "provider_proof" &&
        ["acknowledged", "released"].includes(row.status)));
  const selfWrite =
    self &&
    policy.purposeKey === "election_document" &&
    ["elected", "submitted"].includes(row.status);
  if (operation === "read" ? !managerRead && !selfRead : !managerWrite && !selfWrite)
    privateNotFound();
  return scope(row, operation !== "read");
}

export async function authorizeSensitiveAttachmentTarget(
  db: SqlDatabase,
  actor: WorkContext,
  policy: AttachmentPolicy,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope | null> {
  if (!isSensitiveAttachmentTarget(policy.targetType)) return null;
  switch (policy.targetType) {
    case "hr_person":
      return employeePerson(db, actor, policy, targetId, operation);
    case "hr_evaluation_assessment": {
      const evaluation = await import("../people-evaluation");
      actor = await evaluation.evaluationAuthority(db, actor, operation === "read" ? "people.read" : "people.manage");
      if (!hasPermission(actor, "people.compensation.read") || !hasPermission(actor, operation === "read" ? "people.documents.read" : "people.documents.manage")) privateNotFound();
      const { row, employee } = await evaluation.evaluationAssessment(db, actor, targetId);
      const cycle = await evaluation.evaluationDefinition(db, actor, row.cycle_id, "cycle");
      if (operation !== "read" && (!["draft", "calibrating"].includes(row.status) || cycle.data.status !== "open")) privateNotFound();
      return { companyId: actor.companyId, branchId: employee.branchId, departmentId: employee.departmentId, version: row.version, editable: operation !== "read" };
    }
    case "people_lifecycle_case":
      return lifecycleCase(db, actor, targetId, operation);
    case "people_clearance_task":
      return clearanceTask(db, actor, targetId, operation);
    case "payroll_run":
      return payrollRun(db, actor, targetId, operation);
    case "payroll_advance":
      return payrollAdvance(db, actor, targetId, operation);
    case "payroll_final_pay":
      return finalPay(db, actor, targetId, operation);
    case "talent_candidate":
      return talentCandidate(db, actor, targetId, operation);
    case "talent_offer":
      return talentOffer(db, actor, targetId, operation);
    case "learning_course":
      return learningCourse(db, actor, targetId, operation);
    case "learning_assignment":
      return learningAssignment(db, actor, targetId, operation);
    case "benefit_plan":
      return benefitPlan(db, actor, targetId, operation);
    case "benefit_dependent":
      return benefitDependent(db, actor, targetId, operation);
    case "benefit_election":
      return benefitElection(db, actor, policy, targetId, operation);
    default:
      return null;
  }
}

const sensitiveTables: Record<string, string> = {
  hr_evaluation_assessment: "hr_evaluation_assessments",
  hr_person: "hr_people",
  people_lifecycle_case: "people_lifecycle_cases",
  people_clearance_task: "people_preparation_tasks",
  payroll_run: "hr_payroll_runs",
  payroll_advance: "hr_payroll_advances",
  payroll_final_pay: "hr_payroll_final_pay_preparations",
  talent_candidate: "talent_candidates",
  talent_offer: "talent_offers",
  learning_course: "learning_courses",
  learning_assignment: "learning_assignments",
  benefit_plan: "benefit_plans",
  benefit_dependent: "benefit_dependents",
  benefit_election: "benefit_elections",
};

export function sensitiveAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
  const table = sensitiveTables[targetType];
  if (!table) return null;
  return assertion(
    db,
    commandId,
    `sensitive-attachment-owner-${index}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
    [targetId, companyId, version],
  );
}

const sensitiveRecordTables = new Set([
  "hr_evaluation_assessment",
  "hr_person",
  "people_lifecycle_case",
  "payroll_advance",
  "payroll_final_pay",
  "talent_candidate",
  "talent_offer",
  "learning_course",
  "learning_assignment",
  "benefit_plan",
  "benefit_dependent",
  "benefit_election",
]);

export function sensitiveAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now = Date.now(),
): SqlStatement | null {
  if (!sensitiveRecordTables.has(targetType)) return null;
  const table = sensitiveTables[targetType];
  return db
    .prepare(`UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?`)
    .bind(now, targetId, expectedVersion);
}
