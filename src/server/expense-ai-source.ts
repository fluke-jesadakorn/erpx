import {samePerson} from "@/lib/iam";
import { iamMutationGuards } from "./iam-store";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { assertion } from "./commands";
import { approvalView } from "./approvals";
import { expenseRoleUsers } from "./expense-routing";
import { sha256 } from "./crypto";
import type { StaffActor } from "./staff-policy";
export type AiClaim = Record<string, string | number | null> & {
  id: string;
  company_id: string;
  workflow_stage: string;
  workflow_version: number;
  version: number;
  claimant_user_id: string;
  created_by: string;
  prepared_by: string;
  branch_id: string | null;
  department_id: string | null;
  accounting_json: string;
};
export type EvidencePage = {
  page: number;
  key: string;
  checksum: string;
  bytes: number;
};
export type EvidenceDocument = {
  id: string;
  name: string;
  type: string;
  purpose: string;
  key: string;
  checksum: string;
  bytes: number;
  pageCount: number | null;
  pages: EvidencePage[];
};
export async function aiClaim(db: SqlDatabase, companyId: string, id: string) {
  const row = await db
    .prepare(
      "SELECT * FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2",
    )
    .bind(id, companyId)
    .first<AiClaim>();
  if (!row)
    throw new ApiError(404, "EXPENSE_NOT_FOUND", "Expense claim not found.");
  return row;
}
export async function expenseAiAllowed(
  db: SqlDatabase,
  actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId" | "authority" | "iam">,
  claim: AiClaim,
  paymentId: string | null,
) {
  const authorizationKey=crypto.randomUUID();try{await db.batch([...iamMutationGuards(db,actor,authorizationKey),db.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(authorizationKey)]);}catch{return false;}

  if (
    !(await db
      .prepare(
        "SELECT 1 FROM assignments WHERE company_id=? AND id=? AND user_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?",
      )
      .bind(actor.companyId, actor.assignmentId, actor.userId, actor.role, actor.authority?.assignmentVersion ?? null, actor.authority?.assignmentVersion ?? null, Date.now(), Date.now())
      .first())
  )
    return false;
  if (claim.cancelled_at || claim.company_id !== actor.companyId) return false;
  if (claim.workflow_stage === "draft")
    return (
      !paymentId &&
      [claim.claimant_user_id, claim.created_by].some(id=>samePerson(actor,id))
    );
  if (
    ["department_approval", "accounts_review", "accounts_approval"].includes(
      claim.workflow_stage,
    )
  )
    return (
      !paymentId &&
      (await approvalView(db, "expense", claim.id, actor)).canDecide
    );
  if (claim.workflow_stage !== "payment" || !paymentId) return false;
  const payment = await db
    .prepare(
      "SELECT 1 FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'",
    )
    .bind(paymentId, claim.id, actor.companyId)
    .first();
  const decisions = (
    await db
      .prepare(
        "SELECT s.approver_user_id FROM approval_steps s JOIN approval_instances i ON i.id=s.instance_id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND s.state='approved' AND i.cycle=(SELECT MAX(cycle) FROM approval_instances WHERE record_type='expense' AND record_id=?)",
      )
      .bind(claim.id, actor.companyId, claim.id)
      .all<{ approver_user_id: string }>()
  ).results;
  const forbidden = [
    claim.claimant_user_id,
    claim.created_by,
    claim.prepared_by,
    (JSON.parse(claim.accounting_json) as { preparedBy?: string }).preparedBy,
    ...decisions.map((s) => s.approver_user_id),
  ];
  return (
    !!payment && hasPermission(actor,"expense.pay") &&
    !forbidden.some(id=>samePerson(actor,id)) &&
    (await expenseRoleUsers(db, claim, "treasury")).includes(actor.userId)
  );
}
const evidenceSql = `SELECT erpx_json_array_agg(jsonb_build_object('id',a.id,'name',a.original_name,'type',a.media_type,'purpose',a.owner_type,'key',a.object_key,'checksum',a.checksum_sha256,'bytes',a.bytes,
 'pageCount',(SELECT page_count FROM attachment_documents WHERE attachment_id=a.id AND original_checksum=a.checksum_sha256),
 'pages',erpx_json((SELECT erpx_json_array_agg(jsonb_build_object('page',page_number,'key',object_key,'checksum',checksum_sha256,'bytes',bytes)) FROM (SELECT * FROM attachment_derivatives WHERE attachment_id=a.id AND original_checksum=a.checksum_sha256 ORDER BY page_number)))
 )) manifest FROM (SELECT * FROM attachments WHERE revoked_at IS NULL AND ((owner_type='expense_claim' AND owner_id=?) OR (owner_type='expense_payment' AND owner_id=?)) ORDER BY id) a`;
export async function expenseAiSource(
  db: SqlDatabase,
  companyId: string,
  id: string,
  paymentId: string | null = null,
) {
  const claim = await aiClaim(db, companyId, id);
  const payment = paymentId
    ? await db
        .prepare(
          "SELECT * FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'",
        )
        .bind(paymentId, id, companyId)
        .first<Record<string, string | number | null>>()
    : null;
  if (paymentId && !payment)
    throw new ApiError(
      409,
      "PAYMENT_CHANGED",
      "The payment preparation changed.",
    );
  const raw = (await db
    .prepare(evidenceSql)
    .bind(id, paymentId)
    .first<{ manifest: string }>())!.manifest;
  const evidence = JSON.parse(raw) as EvidenceDocument[];
  const digest = await sha256(
    JSON.stringify({
      raw,
      version: claim.version,
      paymentVersion: payment?.version ?? null,
    }),
  );
  return {
    claim,
    payment,
    evidence,
    digest,
    sourceVersion: Number(payment?.version ?? claim.version),
    guards: (key: string) => [
      assertion(
        db,
        key,
        "ai-claim-source",
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND workflow_stage=? AND version=? AND cancelled_at IS NULL)",
        [id, companyId, claim.workflow_stage, claim.version],
      ),
      ...(payment
        ? [
            assertion(
              db,
              key,
              "ai-payment-source",
              "EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND version=? AND status='draft')",
              [paymentId, Number(payment.version)],
            ),
          ]
        : []),
      assertion(db, key, "ai-evidence-source", `(${evidenceSql})=?`, [
        id,
        paymentId,
        raw,
      ]),
    ],
  };
}
