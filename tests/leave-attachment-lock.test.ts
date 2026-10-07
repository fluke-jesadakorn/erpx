import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkContext } from "../src/server/authorization";
import { applyMigrations, env } from "./runtime-env";

vi.mock("../src/server/modules/files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "ClamAV test", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "ClamAV test", definitionUpdatedAt: Date.now() }),
}));
vi.mock("../src/server/modules/files/public/attachments/preview", () => ({
  attachmentPreviewReadiness: async () => ({ ready: true, heifConvert: "/test/heif-convert", pdftoppm: "/test/pdftoppm", error: null }),
  detectedAttachmentMediaType: (_bytes: Uint8Array, declared: string) => declared,
  sanitizedAttachmentPreview: async () => null,
}));

import {
  cleanupAttachmentDrafts,
  completeAttachmentUpload,
  createAttachmentSet,
  linkExistingAttachment,
  reserveAttachmentFile,
  withdrawAttachmentLink,
} from "../src/server/attachments/service";

const db = env.H001_DB;
const key = () => crypto.randomUUID();
const pdf = new TextEncoder().encode("%PDF-1.4\n%%EOF\n");
let company: string;
let requester: WorkContext;

function actor(companyId: string, userId: string): WorkContext {
  return {
    userId,
    name: "Leave requester",
    email: `${userId}@example.test`,
    assignmentId: `assignment-${userId}`,
    role: "holding_owner",
    companyId,
    branchId: null,
    departmentId: null,
    holdingId: "h001",
    holdingSlug: "acme",
    holdingName: "Test",
    scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "Test", companyId },
    assignmentScope: "holding",
    inherited: false,
    modules: [],
    assignments: [],
    workspaces: [],
  };
}

async function leaveRequest(state = "pending_approval", version = 1) {
  const id = `leave-lock-${key()}`;
  await db.prepare(`INSERT INTO leave_requests(
    id,number,employee_id,company_id,branch_id,start_date,days,state,version,created_by,created_at,updated_at
  ) VALUES(?,?,?,?,?,'2026-10-01',1,?,?,?,1,1)`)
    .bind(id, `LEAVE-${key()}`, `employee-${requester.userId}`, company, `branch-${company}`, state, version, requester.userId).run();
  return id;
}

async function stagedReservation(requestId: string, expectedTargetVersion = 1) {
  const set = await createAttachmentSet(db, requester, {
    targetType: "leave_request",
    targetId: requestId,
    componentKind: "record",
    componentRef: "leave-evidence:medical_certificate",
    purposeKey: "medical_certificate",
    expectedTargetVersion,
  }, key());
  const reservation = await reserveAttachmentFile(db, requester, set.id, {
    filename: "certificate.pdf",
    mediaType: "application/pdf",
    bytes: pdf.byteLength,
  }, key());
  return { set, reservation };
}

async function waitUntilBlockedBy(blockerPid: number) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const blocked = await db.prepare(`SELECT EXISTS(
      SELECT 1 FROM pg_stat_activity activity
      WHERE ?::int=ANY(pg_blocking_pids(activity.pid))
    ) AS blocked`).bind(blockerPid).first<boolean>("blocked");
    if (blocked) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("ATTACHMENT_MUTATION_DID_NOT_WAIT_FOR_LEAVE_LOCK");
}

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  process.env.ERPX_ATTACHMENTS_V2 = "true";
  delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
});

afterAll(() => {
  delete process.env.ERPX_ATTACHMENTS_V2;
  delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
});

beforeEach(async () => {
  company = `leave-attachment-company-${key()}`;
  requester = actor(company, `leave-requester-${key()}`);
  const branch=`branch-${company}`;
  await db.batch([
    db.prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)").bind(company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Leave attachment Company',?)").bind(company, company, company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Leave attachment Branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,?,?,'holding','[]',0,9999999999999)")
      .bind(requester.assignmentId, requester.userId, requester.role, company),
    db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,display_name,status,created_at) VALUES(?,?,?,?,?,'active',1)")
      .bind(`employee-${requester.userId}`,requester.userId,company,branch,"Leave requester"),
  ]);
});

describe("Leave attachment serialization", () => {
  it("rechecks every staged mutation after final approval and removes rejected quarantine bytes", async () => {
    const requestId = await leaveRequest();
    const { set, reservation } = await stagedReservation(requestId);
    await db.prepare("UPDATE leave_requests SET state='approved',version=2,updated_at=2 WHERE id=?").bind(requestId).run();

    await expect(createAttachmentSet(db, requester, {
      targetType: "leave_request",
      targetId: requestId,
      componentKind: "record",
      purposeKey: "medical_certificate",
      expectedTargetVersion: 2,
    }, key())).rejects.toMatchObject({ status: 403, code: "LEAVE_EVIDENCE_REQUESTER_REQUIRED" });
    await expect(reserveAttachmentFile(db, requester, set.id, {
      filename: "late.pdf",
      mediaType: "application/pdf",
      bytes: pdf.byteLength,
    }, key())).rejects.toMatchObject({ status: 403, code: "LEAVE_EVIDENCE_REQUESTER_REQUIRED" });
    await expect(linkExistingAttachment(db, requester, set.id, { sourceLinkId: "unreachable-source" }, key()))
      .rejects.toMatchObject({ status: 403, code: "LEAVE_EVIDENCE_REQUESTER_REQUIRED" });
    await expect(withdrawAttachmentLink(db, requester, reservation.attachmentLinkId, "Final approval already recorded", key()))
      .rejects.toMatchObject({ status: 403, code: "LEAVE_EVIDENCE_REQUESTER_REQUIRED" });
    await expect(completeAttachmentUpload(db, env.H001_FILES, requester, reservation.uploadIntentId, pdf.buffer.slice(pdf.byteOffset, pdf.byteOffset + pdf.byteLength)))
      .rejects.toMatchObject({ status: 403, code: "LEAVE_EVIDENCE_REQUESTER_REQUIRED" });

    expect(await db.prepare("SELECT attachment_id,state FROM attachment_links WHERE id=?").bind(reservation.attachmentLinkId).first())
      .toEqual({ attachment_id: null, state: "draft" });
    expect(await db.prepare("SELECT count(*)::int count FROM attachments WHERE upload_id=?").bind(reservation.uploadIntentId).first("count")).toBe(0);
    expect(await env.H001_FILES.head(`h001/quarantine/${reservation.uploadIntentId}/original`)).toBeNull();
  });

  it("serializes a create against final approval and leaves no stale set", async () => {
    const requestId = await leaveRequest();
    let releaseApproval!: () => void;
    let reportLock!: (pid: number) => void;
    const locked = new Promise<number>((resolve) => { reportLock = resolve; });
    const release = new Promise<void>((resolve) => { releaseApproval = resolve; });
    const approval = db.transaction(async (tx) => {
      const pid = Number(await tx.prepare("SELECT pg_backend_pid() pid").first("pid"));
      await tx.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE").bind(requestId, company).run();
      reportLock(pid);
      await release;
      await tx.prepare("UPDATE leave_requests SET state='approved',version=2,updated_at=2 WHERE id=?").bind(requestId).run();
    });
    const blockerPid = await locked;
    const mutation = createAttachmentSet(db, requester, {
      targetType: "leave_request",
      targetId: requestId,
      componentKind: "record",
      purposeKey: "medical_certificate",
      expectedTargetVersion: 1,
    }, key()).then(value => ({ value, error: null }), error => ({ value: null, error }));
    try {
      await waitUntilBlockedBy(blockerPid);
    } finally {
      releaseApproval();
    }
    await approval;
    const outcome = await mutation;
    expect(outcome.error).toMatchObject({ status: 403, code: "LEAVE_EVIDENCE_REQUESTER_REQUIRED" });
    expect(outcome.value).toBeNull();
    expect(await db.prepare("SELECT count(*)::int count FROM attachment_sets WHERE target_type='leave_request' AND target_id=?").bind(requestId).first("count")).toBe(0);
  });

  it("never attaches completed bytes to a set that is already bound", async () => {
    const requestId = await leaveRequest();
    const { set, reservation } = await stagedReservation(requestId);
    await db.prepare("UPDATE attachment_sets SET state='bound',bound_at=2,bound_command_id='already-bound',updated_at=2 WHERE id=?").bind(set.id).run();

    await expect(completeAttachmentUpload(db, env.H001_FILES, requester, reservation.uploadIntentId, pdf.buffer.slice(pdf.byteOffset, pdf.byteOffset + pdf.byteLength)))
      .rejects.toMatchObject({ status: 409, code: "ATTACHMENT_SET_LOCKED" });
    expect(await db.prepare("SELECT attachment_id,state FROM attachment_links WHERE id=?").bind(reservation.attachmentLinkId).first())
      .toEqual({ attachment_id: null, state: "draft" });
    expect(await db.prepare("SELECT count(*)::int count FROM attachments WHERE upload_id=?").bind(reservation.uploadIntentId).first("count")).toBe(0);
    expect(await env.H001_FILES.head(`h001/quarantine/${reservation.uploadIntentId}/original`)).toBeNull();
  });

  it("replays the same completed PUT after binding without reopening the set", async () => {
    const requestId = await leaveRequest();
    const { set, reservation } = await stagedReservation(requestId);
    const content = pdf.buffer.slice(pdf.byteOffset, pdf.byteOffset + pdf.byteLength);
    const completed = await completeAttachmentUpload(db, env.H001_FILES, requester, reservation.uploadIntentId, content);
    await db.batch([
      db.prepare("UPDATE attachment_links SET state='bound',bound_at=2,binding_type='leave_approval',binding_id='approval-1' WHERE id=?").bind(reservation.attachmentLinkId),
      db.prepare("UPDATE attachment_sets SET state='bound',bound_at=2,bound_command_id='approval-1',updated_at=2 WHERE id=?").bind(set.id),
      db.prepare("UPDATE leave_requests SET state='approved',version=2,updated_at=2 WHERE id=?").bind(requestId),
    ]);

    await expect(completeAttachmentUpload(db, env.H001_FILES, requester, reservation.uploadIntentId, content)).resolves.toEqual(completed);
    expect(await db.prepare("SELECT count(*)::int count FROM attachments WHERE upload_id=?").bind(reservation.uploadIntentId).first("count")).toBe(1);
    expect(await db.prepare("SELECT state,attachment_id FROM attachment_links WHERE id=?").bind(reservation.attachmentLinkId).first())
      .toEqual({ state: "bound", attachment_id: completed.sourceId });
  });

  it("lets a winning Leave decision bind before cleanup evaluates expiry", async () => {
    const requestId = await leaveRequest();
    const { set, reservation } = await stagedReservation(requestId);
    await db.prepare("UPDATE attachment_links SET expires_at=0 WHERE id=?").bind(reservation.attachmentLinkId).run();
    let releaseDecision!: () => void;
    let reportLock!: (pid: number) => void;
    const locked = new Promise<number>((resolve) => { reportLock = resolve; });
    const release = new Promise<void>((resolve) => { releaseDecision = resolve; });
    const decision = db.transaction(async (tx) => {
      const pid = Number(await tx.prepare("SELECT pg_backend_pid() pid").first("pid"));
      await tx.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE").bind(requestId, company).run();
      reportLock(pid);
      await release;
      await tx.batch([
        tx.prepare("UPDATE attachment_links SET state='bound',bound_at=2,binding_type='leave_approval',binding_id='approval-cleanup-race' WHERE id=? AND state='draft'").bind(reservation.attachmentLinkId),
        tx.prepare("UPDATE attachment_sets SET state='bound',bound_at=2,bound_command_id='approval-cleanup-race',updated_at=2 WHERE id=? AND state='draft'").bind(set.id),
        tx.prepare("UPDATE leave_requests SET state='approved',version=2,updated_at=2 WHERE id=?").bind(requestId),
      ]);
    });
    const blockerPid = await locked;
    const cleanup = cleanupAttachmentDrafts(db, env.H001_FILES);
    try {
      await waitUntilBlockedBy(blockerPid);
    } finally {
      releaseDecision();
    }
    await decision;
    const result = await cleanup;

    expect(result.expired).toBe(0);
    expect(result.expiredSets).toBe(0);
    expect(await db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(reservation.attachmentLinkId).first("state")).toBe("bound");
    expect(await db.prepare("SELECT state FROM attachment_sets WHERE id=?").bind(set.id).first("state")).toBe("bound");
  });

  it("expires an abandoned Leave reservation inside the owner lock", async () => {
    const requestId = await leaveRequest();
    const { set, reservation } = await stagedReservation(requestId);
    await db.batch([
      db.prepare("UPDATE attachment_links SET expires_at=0 WHERE id=?").bind(reservation.attachmentLinkId),
      db.prepare("UPDATE upload_intents SET expires_at=0 WHERE id=?").bind(reservation.uploadIntentId),
    ]);

    const result = await cleanupAttachmentDrafts(db, env.H001_FILES);
    expect(result).toMatchObject({ expired: 1, expiredSets: 1 });
    expect(await db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(reservation.attachmentLinkId).first("state")).toBe("expired");
    expect(await db.prepare("SELECT state FROM attachment_sets WHERE id=?").bind(set.id).first("state")).toBe("expired");
    expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(reservation.uploadIntentId).first("status")).toBe("expired");
  });

  it("revalidates the active session and assignment under lock", async () => {
    const requestId = await leaveRequest();
    const sessionId = `session-${key()}`;
    const current = { ...requester, authority: { sessionId, revision: 0, assignmentVersion: 1 } };
    await db.prepare(`INSERT INTO active_work_contexts(
      session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at
    ) VALUES(?,?,?,?,1,0,9999999999999,1)`).bind(sessionId,requester.userId,company,requester.assignmentId).run();
    const set = await createAttachmentSet(db, current, {
      targetType: "leave_request",
      targetId: requestId,
      componentKind: "record",
      purposeKey: "medical_certificate",
      expectedTargetVersion: 1,
    }, key());

    await db.prepare("UPDATE active_work_contexts SET revision=1 WHERE session_id=? AND company_id=?").bind(sessionId,company).run();
    await expect(reserveAttachmentFile(db, current, set.id, {
      filename: "stale-session.pdf",
      mediaType: "application/pdf",
      bytes: pdf.byteLength,
    }, key())).rejects.toMatchObject({ status: 409, code: "WORK_CONTEXT_CHANGED" });

    await db.batch([
      db.prepare("UPDATE active_work_contexts SET revision=0 WHERE session_id=? AND company_id=?").bind(sessionId,company),
      db.prepare("UPDATE assignments SET revoked_at=2 WHERE id=?").bind(requester.assignmentId),
    ]);
    await expect(reserveAttachmentFile(db, current, set.id, {
      filename: "revoked-assignment.pdf",
      mediaType: "application/pdf",
      bytes: pdf.byteLength,
    }, key())).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
    expect(await db.prepare("SELECT count(*)::int count FROM attachment_links WHERE set_id=?").bind(set.id).first("count")).toBe(0);
  });
});
