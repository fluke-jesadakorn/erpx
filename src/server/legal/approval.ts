import "server-only";
import { z } from "zod";
import type { SqlDatabase, SqlStatement } from "../runtime/types";
import type { WorkContext } from "../authorization";
import type { DocumentVersion, LegalItem } from "@/lib/legal/model";
import { openApproval, currentApproval, type ApprovalStep } from "../approvals";
import { access, guards, eligible, personGuard, event, fail } from "./access";
import { executeCommand, replayCommand, assertion } from "../commands";
import { samePerson } from "@/lib/iam";
import { hash } from "./service";
export function approvalDigest(
  item: LegalItem,
  documentId: string,
  checksum: string,
  signers: string[],
) {
  const { version: _version, updated_at: _updated, ...source } = item;
  void _version;
  void _updated;
  return hash(JSON.stringify({ source, documentId, checksum, signers }));
}
export async function legalApprovalView(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
) {
  await access(db, a, id);
  const instance = await currentApproval(db, "legal", id, a.companyId);
  return {
    instance,
    steps: instance
      ? (
          await db
            .prepare(
              "SELECT * FROM approval_steps WHERE instance_id=? ORDER BY ordinal",
            )
            .bind(instance.id)
            .all()
        ).results
      : [],
    history: (
      await db
        .prepare(
          "SELECT d.* FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_type='legal' AND i.record_id=? AND i.company_id=? ORDER BY d.insertion_order",
        )
        .bind(id, a.companyId)
        .all()
    ).results,
  };
}
export async function legalApprovalCommand(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum(["approval.open", "approval.decide", "approval.reroute"]),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  const item = await access(
    db,
    a,
    x.id,
    x.action === "approval.open" ? "legal.review" : "legal.approve",
  );
  const replay = await replayCommand(db, a, key, x.action, item.id, raw);
  if (replay) return replay;
  if (item.kind !== "contract") fail("Contract approval requires a contract.");
  const steps: SqlStatement[] = guards(
    db,
    a,
    item.id,
    x.expectedVersion,
    key,
    false,
  );
  let state = item.state;
  if (x.action === "approval.open") {
    if (!["draft", "review", "approval"].includes(item.state))
      fail("Return this contract to draft before opening a new approval.");
    const p = z
      .object({
        documentId: z.string(),
        signerIds: z.array(z.string()).min(1).max(20),
        approverIds: z.array(z.string()).min(1).max(10),
      })
      .parse(raw);
    if (
      new Set(p.signerIds).size !== p.signerIds.length ||
      new Set(p.approverIds).size !== p.approverIds.length
    )
      fail("Signers and approvers must be unique.");
    const d = await db
      .prepare("SELECT * FROM legal_documents WHERE id=? AND item_id=?")
      .bind(p.documentId, item.id)
      .first<DocumentVersion>();
    if (!d) fail("Select a version on this contract.");
    if(await db.prepare("SELECT 1 FROM legal_executed_copies WHERE document_id=?").bind(d.id).first())fail("Imported executed copies are reviewed separately from draft approval.");
    const author = await db
      .prepare(
        "SELECT actor_id FROM legal_events WHERE item_id=? AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC LIMIT 1",
      )
      .bind(item.id)
      .first<{ actor_id: string }>();
    for (const u of p.approverIds) {
      await reviewer(db, a, item, u);
      steps.push(personGuard(db,a,key,u,item,"legal.approve"),assertion(db,key,`legal-reviewer-membership-${u}`,"EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[item.id,u]));
      if (
        await isAuthor(db, a.companyId, u, [
          a.userId,
          item.created_by,
          d.created_by,
          author?.actor_id,
        ])
      )
        fail(
          "Content authors cannot approve their own work.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
    }
    for (const u of p.signerIds)
      if (
        !(await db
          .prepare(
            "SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? UNION ALL SELECT 1 FROM legal_shares WHERE item_id=? AND document_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?",
          )
          .bind(item.id, u, item.id, d.id, u, Date.now())
          .first())
      )
        fail("Each signer must have explicit access to this version.");
    const opened = await openApproval(
      db,
      key,
      "legal",
      item.id,
      "request",
      a.userId,
      null,
      {
        scope: {
          company_id: item.company_id,
          branch_id: item.branch_id,
          department_id: item.department_id,
        },
        steps: p.approverIds.map((userId) => ({
          userId,
          authority: "named" as const,
          snapshot: {
            documentId: d.id,
            checksum: d.checksum,
            signers: p.signerIds,
          },
        })),
        blockedReason: null,
      },
      item.version + 1,
    );
    const after = { ...item, state: "approval" };
    steps.push(
      ...opened.statements,
      db
        .prepare("UPDATE legal_documents SET locked=true WHERE id=?")
        .bind(d.id),
      db
        .prepare(
          "INSERT INTO legal_approval_routes(item_id,instance_id,document_id,document_hash,signers,source_version,created_by,source_hash) VALUES(?,?,?,?,?::jsonb,?,?,?) ON CONFLICT(item_id) DO UPDATE SET instance_id=excluded.instance_id,document_id=excluded.document_id,document_hash=excluded.document_hash,signers=excluded.signers,source_version=excluded.source_version,created_by=excluded.created_by,source_hash=excluded.source_hash",
        )
        .bind(
          item.id,
          opened.instanceId,
          d.id,
          d.checksum,
          JSON.stringify(p.signerIds),
          item.version + 1,
          a.userId,
          approvalDigest(after, d.id, d.checksum, p.signerIds),
        ),
    );
    state = "approval";
  } else {
    const instance = await currentApproval(db, "legal", item.id, a.companyId);
    if (!instance || instance.state !== "pending" || item.state !== "approval")
      fail("No active approval route.");
    const current = await db
      .prepare("SELECT * FROM approval_steps WHERE instance_id=? AND ordinal=?")
      .bind(instance.id, instance.current_step)
      .first<ApprovalStep>();
    if (!current) fail("Approval step unavailable.");
    const route = await db
      .prepare(
        "SELECT * FROM legal_approval_routes WHERE item_id=? AND instance_id=?",
      )
      .bind(item.id, instance.id)
      .first<{
        document_id: string;
        document_hash: string;
        signers: string;
        source_hash: string;
      }>();
    if (!route) fail("Approval snapshot unavailable.");
    const signers =
      typeof route.signers === "string"
        ? (JSON.parse(route.signers) as string[])
        : route.signers;
    if (
      approvalDigest(item, route.document_id, route.document_hash, signers) !==
      route.source_hash
    )
      fail(
        "Material contract data changed. Open a fresh approval.",
        "LEGAL_APPROVAL_STALE",
        409,
      );
    steps.push(
      assertion(
        db,
        key,
        "legal-approval-current",
        "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND state='pending' AND current_step=? AND version=?)",
        [instance.id, instance.current_step, instance.version],
      ),
    );
    if (x.action === "approval.reroute") {
      const p = z.object({ approverId: z.string() }).parse(raw);
      await reviewer(db, a, item, p.approverId);
      steps.push(personGuard(db,a,key,p.approverId,item,"legal.approve"),assertion(db,key,"legal-reroute-membership","EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[item.id,p.approverId]));
      const author = await db
        .prepare("SELECT created_by FROM legal_documents WHERE id=?")
        .bind(route.document_id)
        .first<{ created_by: string }>();
      if (
        await isAuthor(db, a.companyId, p.approverId, [
          item.created_by,
          instance.requester_user_id,
          author?.created_by,
        ])
      )
        fail("The replacement must be independent.");
      steps.push(
        db
          .prepare(
            "UPDATE approval_instances SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(Date.now(), instance.id),
        db
          .prepare("UPDATE approval_steps SET approver_user_id=? WHERE id=?")
          .bind(p.approverId, current.id),
        db
          .prepare(
            "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,previous_approver_user_id,approver_user_id) VALUES(?,?,?,'reroute',?,?,?,?,?,?,?)",
          )
          .bind(
            key,
            instance.id,
            current.id,
            a.userId,
            x.reason,
            item.version,
            key,
            Date.now(),
            current.approver_user_id,
            p.approverId,
          ),
      );
    } else {
      const decision = z
        .enum(["approve", "reject", "revise"])
        .parse((raw as { decision: unknown }).decision);
      if (!samePerson(a, current.approver_user_id))
        fail(
          "This decision is assigned to another reviewer.",
          "LEGAL_APPROVER",
          403,
        );
      await reviewer(db, a, item, a.userId);
      steps.push(personGuard(db,a,key,a.userId,item,"legal.approve"),assertion(db,key,"legal-decision-membership","EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[item.id,a.userId]));
      if (
        samePerson(a, item.created_by) ||
        samePerson(a, instance.requester_user_id)
      )
        fail(
          "An independent reviewer is required.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(
        db
          .prepare("UPDATE approval_steps SET state=? WHERE id=?")
          .bind(decision === "approve" ? "approved" : "rejected", current.id),
        db
          .prepare(
            "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            key,
            instance.id,
            current.id,
            decision,
            a.userId,
            x.reason,
            item.version,
            key,
            Date.now(),
            JSON.stringify({
              role: a.role,
              assignmentId: a.assignmentId,
              documentId: route.document_id,
              documentHash: route.document_hash,
            }),
          ),
      );
      const next = await db
        .prepare(
          "SELECT approver_user_id FROM approval_steps WHERE instance_id=? AND ordinal=?",
        )
        .bind(instance.id, instance.current_step + 1)
        .first<{ approver_user_id: string }>();
      if (decision !== "approve") {
        state = decision === "revise" ? "draft" : "rejected";
        steps.push(
          db
            .prepare(
              "UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              decision === "revise" ? "cancelled" : "rejected",
              Date.now(),
              instance.id,
            ),
        );
      } else if (next) {
        steps.push(
          db
            .prepare(
              "UPDATE approval_instances SET current_step=current_step+1,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(Date.now(), instance.id),
          db
            .prepare(
              "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'legal',?,?,'Legal approval needs your decision',?)",
            )
            .bind(
              crypto.randomUUID(),
              a.companyId,
              next.approver_user_id,
              item.id,
              item.version + 1,
              Date.now(),
            ),
        );
      } else {
        state = "approved";
        steps.push(
          db
            .prepare(
              "UPDATE approval_instances SET state='approved',version=version+1,updated_at=? WHERE id=?",
            )
            .bind(Date.now(), instance.id),
          db
            .prepare(
              "INSERT INTO legal_approvals VALUES(?,?,?,?,?::jsonb,?,'approved',?)",
            )
            .bind(
              item.id,
              item.version + 1,
              route.document_id,
              route.document_hash,
              JSON.stringify(signers),
              a.userId,
              Date.now(),
            ),
        );
      }
    }
  }
  steps.push(
    db
      .prepare(
        "UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(state, Date.now(), item.id),
    event(db, a, item.id, x.action, x.reason, item.version + 1),
  );
  return executeCommand(
    db,
    a,
    key,
    x.action,
    raw,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1, state },
    steps,
  );
}
async function reviewer(
  db: SqlDatabase,
  a: WorkContext,
  item: LegalItem,
  id: string,
) {
  await eligible(db, a, id,item,"legal.approve");
  if (
    !(await db
      .prepare(
        "SELECT 1 FROM legal_members m JOIN assignments a ON a.user_id=m.user_id AND a.company_id=? WHERE m.item_id=? AND m.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_role_allows(a.role,a.company_id,'legal.approve')",
      )
      .bind(item.company_id, item.id, id, Date.now(), Date.now())
      .first())
  )
    fail(
      "Reviewer needs explicit record membership and an active Legal approval role.",
    );
}

async function isAuthor(
  db: SqlDatabase,
  company: string,
  user: string,
  authors: (string | undefined)[],
) {
  for (const author of authors)
    if (
      author &&
      (await db
        .prepare("SELECT 1 WHERE iam_same_person(?,?,?)")
        .bind(user, author, company)
        .first())
    )
      return true;
  return false;
}
