import {peoplePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import { hospitalityOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/hospitality-owner-projection-sources";
import { expenseNativeEffects as workflowExpenseEffects } from "@erpx/server-workflow/public/expense-native-effects";
import { createExpenseAiAllowed, createExpenseAiSource, createExpenseAiClaim, type AiClaim, type ExpenseAiActor } from "@erpx/server-people_payroll/application/expense-ai-source";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {approvalView} from '@erpx/server-workflow/public/approval-planning';
import { expenseRoleUsers } from "@erpx/server-people_payroll/adapters/expense-routing";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import {
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "@erpx/server-files/public/attachments/feature";
export type { AiClaim, EvidenceDocument, EvidencePage } from "@erpx/server-people_payroll/application/expense-ai-source";
export function nativeExpenseAiClaim(db: SqlDatabase) {
  return createExpenseAiClaim({
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    claim: (companyId, id) => db.prepare("SELECT * FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2").bind(id, companyId).first<AiClaim>(),
  });
}
export async function aiClaim(db: SqlDatabase, companyId: string, id: string) {
  await checkPurchasedCapability(db,await peoplePackageEnforcementMode(),{companyId,capability:"people.expenses",purpose:"history"});
  return nativeExpenseAiClaim(db)(companyId, id);
}
export function nativeExpenseAiAllowed(db: SqlDatabase) {
  return createExpenseAiAllowed({
    async currentActor(actor) {
      const authorizationKey = crypto.randomUUID();
      try { await db.batch([...iamMutationGuards(db, actor, authorizationKey), workflowExpenseEffects(db).clearAssertions({commandId:authorizationKey})]); } catch { return false; }
      return Boolean(await db.prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND id=? AND user_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?`)
        .bind(actor.companyId, actor.assignmentId, actor.userId, actor.role, actor.authority?.assignmentVersion ?? null, actor.authority?.assignmentVersion ?? null, Date.now(), Date.now()).first());
    },
    async canDecide(actor, claim) { return (await approvalView(db, "expense", claim.id, actor)).canDecide; },
    async draftPayment(actor, claim, paymentId) { return Boolean(await db.prepare("SELECT 1 FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'").bind(paymentId, claim.id, actor.companyId).first()); },
    async approvedPeople(actor, claim) { return (await db.prepare(`SELECT s.approver_user_id FROM (${workflowProjectionSources.approval_steps}) s JOIN (${workflowProjectionSources.approval_instances}) i ON i.id=s.instance_id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND s.state='approved' AND i.cycle=(SELECT MAX(cycle) FROM (${workflowProjectionSources.approval_instances}) approval_instances WHERE record_type='expense' AND record_id=?)`)
      .bind(claim.id, actor.companyId, claim.id).all<{ approver_user_id: string }>()).results.map(row => row.approver_user_id); },
    treasury: claim => expenseRoleUsers(db, claim, "treasury"),
  });
}
export async function expenseAiAllowed(db: SqlDatabase, actor: ExpenseAiActor, claim: AiClaim, paymentId: string | null,purpose:CapabilityPurpose="new-work") {
  const mode=await peoplePackageEnforcementMode();if(mode!=="disabled"){const access=await readPurchasedAccess(db,mode,{companyId:actor.companyId,capability:"people.expenses"});if(!(purpose==="new-work"?access.newWork:purpose==="settle-existing"?access.settleExisting:access.history))return false;}
  return nativeExpenseAiAllowed(db)(actor, claim, paymentId);
}
function expenseEvidenceSql() {
  const lifecycle = attachmentsV2Readable()
    ? `AND a.source_state='available' AND EXISTS(
         SELECT 1 FROM (${filesProjectionSources.attachment_links}) l JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
         WHERE l.attachment_id=a.id AND l.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
           AND s.company_id=? AND s.target_type=a.owner_type AND s.target_id=a.owner_id
       )`
    : "";
  return `SELECT erpx_json_array_agg(jsonb_build_object('id',a.id,'name',a.original_name,'type',a.media_type,'purpose',a.owner_type,'key',a.object_key,'checksum',a.checksum_sha256,'bytes',a.bytes,
 'pageCount',(SELECT page_count FROM (${filesProjectionSources.attachment_documents}) attachment_documents WHERE attachment_id=a.id AND original_checksum=a.checksum_sha256),
 'pages',erpx_json((SELECT erpx_json_array_agg(jsonb_build_object('page',page_number,'key',object_key,'checksum',checksum_sha256,'bytes',bytes)) FROM (SELECT * FROM (${filesProjectionSources.attachment_derivatives}) attachment_derivatives WHERE attachment_id=a.id AND original_checksum=a.checksum_sha256 ORDER BY page_number)))
 )) manifest FROM (SELECT * FROM (${filesProjectionSources.attachments}) a WHERE revoked_at IS NULL AND ((owner_type='expense_claim' AND owner_id=?) OR (owner_type='expense_payment' AND owner_id=?)) ${lifecycle} ORDER BY id) a`;
}
export async function expenseAiSource(
  db: SqlDatabase,
  companyId: string,
  id: string,
  paymentId: string | null = null,
) {
  let evidenceSql = "", evidenceArgs: (string | null)[] = [];
  const { raw, ...source } = await createExpenseAiSource({
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    claim: (companyId, id) => aiClaim(db, companyId, id),
    manifest(companyId, id, paymentId) {
      evidenceSql = expenseEvidenceSql(); evidenceArgs = [id, paymentId, ...(attachmentsV2Readable() ? [companyId] : [])];
      return { async read() { return (await db.prepare(evidenceSql).bind(...evidenceArgs).first<{ manifest: string }>())!.manifest; } };
    },
    payment: (companyId, id, paymentId) => db.prepare("SELECT * FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'").bind(paymentId, id, companyId).first<Record<string, string | number | null>>(),
    digest: sha256,
  })(companyId, id, paymentId);
  const { claim, payment } = source;
  return {
    ...source,
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
        ...evidenceArgs,
        raw,
      ]),
    ],
  };
}
