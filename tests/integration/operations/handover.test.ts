import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { branchStatements, memberStatements } from "../../../src/server/modules/manage_erpx/public/beta-onboarding";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { bangkokToday, handoverSuggestion } from "../../../src/lib/shared/handover";
import { acknowledgeHandover, handoverAcknowledgement, handoverView, publishHandover } from "../../../src/server/modules/operations/public/handover";
import { postAction } from "../../../src/server/modules/collaboration/public/social-home";
import { readablePost } from "../../../src/server/modules/collaboration/public/social-source";
import type { WorkSnapshot } from "../../../src/server/modules/work_projection/public/work-items";
import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, read: env.H001_READ_DB, files: env.H001_FILES }),
}));
const db = env.H001_DB;
const key = () => crypto.randomUUID();
let company: string, branch: string, owner: WorkContext, receiver: WorkContext, outsider: WorkContext;
let taskId: string, snapshot: WorkSnapshot;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});

beforeEach(async () => {
  company = `co-${key()}`;
  branch = `br-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Test company',?)")
      .bind(company, company, company),
    ...branchStatements(db, company, branch, "Branch", branch),
    db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'member','Member',1,1,1)")
      .bind(`level-${company}`, company),
    db.prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)").bind(company),
  ]);
  async function person(name: string, ownerRole: boolean): Promise<WorkContext> {
    const id = key();
    await db.batch(memberStatements(db, company, branch,
      { id, name, email: `${id}@example.test`, emailVerified: true }, ownerRole, null));
    return {
      userId: id, name, email: `${id}@example.test`, assignmentId: `beta-assignment-${id}`,
      role: ownerRole ? "holding_owner" : "employee", companyId: company,
      branchId: ownerRole ? null : branch, departmentId: null,
      holdingId: "h001", holdingSlug: "acme", holdingName: "Test",
      scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "Test", companyId: company },
      assignmentScope: ownerRole ? "holding" : "branch", inherited: false, modules: [], assignments: [], workspaces: [],
    };
  }
  owner = await person("Owner", true);
  receiver = await person("Receiver", false);
  outsider = await person("Unrelated colleague", false);
  taskId = `task-${key()}`;
  const now = Date.now();
  await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,title,creator_user_id,assignee_user_id,
    due_at,state,created_at,updated_at) VALUES(?,?,?,?,?,? ,NULL,'in_progress',?,?)`)
    .bind(taskId, company, branch, "Repair front desk printer", owner.userId, receiver.userId, now, now).run();
  snapshot = { unavailableSources: [], items: [{
    id: taskId, type: "staff_task", title: "Repair front desk printer", detail: "In progress",
    href: `/staff-tasks/${taskId}`, status: "in_progress", updatedAt: now, queue: "todo",
    nextAction: "Check replacement part", blocker: "Waiting for replacement part",
  }] };
});

describe("Today handover", () => {
  it("suggests an editable Gregorian handover from readable Work items without changing the task", async () => {
    const view = await handoverView(db, owner, snapshot);
    expect(view).toMatchObject({ date: bangkokToday(), unavailable: false, defaultIds: [taskId] });
    expect(view.people.map(person => person.id)).toContain(receiver.userId);
    const body = handoverSuggestion(view.date, view.items);
    expect(body).toContain("Needs help");
    expect(body).toContain("Waiting for replacement part");
    expect(body).toMatch(/\b20\d{2}\b/);
    expect(await db.prepare("SELECT state,version FROM staff_tasks WHERE id=?").bind(taskId)
      .first()).toMatchObject({ state: "in_progress", version: 1 });
  });

  it("requires every intended recipient to open each source, publishes once, and keeps task state", async () => {
    const view = await handoverView(db, owner, snapshot);
    const body = handoverSuggestion(view.date, view.items);
    const input = { date: view.date, body, sources: [{ id: taskId, sourceVersion: 1 }], recipientIds: [outsider.userId] };
    await expect(publishHandover(db, owner, snapshot, input, key()))
      .rejects.toMatchObject({ code: "HANDOVER_RECIPIENT_ACCESS", status: 403 });
    expect(await db.prepare("SELECT count(*) n FROM social_post_drafts WHERE company_id=?")
      .bind(company).first<number>("n")).toBe(0);
    input.recipientIds = [receiver.userId];
    const publishKey = key();
    const result = await publishHandover(db, owner, snapshot, input, publishKey);
    expect(await publishHandover(db, owner, snapshot, input, publishKey)).toEqual(result);
    expect((await readablePost(db, receiver, result.id)).purpose).toBe("handover");
    await expect(readablePost(db, outsider, result.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
    expect(await db.prepare("SELECT count(*) n FROM social_posts WHERE id=?").bind(result.id).first<number>("n")).toBe(1);
    expect(await db.prepare("SELECT state,version FROM staff_tasks WHERE id=?").bind(taskId)
      .first()).toMatchObject({ state: "in_progress", version: 1 });
  });

  it("confirms receipt only by an intended recipient and resets confirmation after an edit", async () => {
    const view = await handoverView(db, owner, snapshot);
    const published = await publishHandover(db, owner, snapshot, {
      date: view.date, body: "Printer repair is waiting for parts.",
      sources: [{ id: taskId, sourceVersion: 1 }], recipientIds: [receiver.userId],
    }, key());
    expect((await handoverAcknowledgement(db, receiver, published.id)).acknowledged).toBe(false);
    await expect(acknowledgeHandover(db, owner, published.id, key()))
      .rejects.toMatchObject({ code: "HANDOVER_RECIPIENT_REQUIRED", status: 403 });
    const receiptKey = key();
    expect((await acknowledgeHandover(db, receiver, published.id, receiptKey)).acknowledged).toBe(true);
    expect((await acknowledgeHandover(db, receiver, published.id, receiptKey)).acknowledgedCount).toBe(1);
    expect(await db.prepare("SELECT count(*) n FROM social_handover_acknowledgements WHERE post_id=?")
      .bind(published.id).first<number>("n")).toBe(1);
    await postAction(db, owner, published.id, { action: "edit", expectedVersion: 1, body: "Printer part is due tomorrow." }, key());
    const changed = await handoverAcknowledgement(db, receiver, published.id);
    expect(changed).toMatchObject({ postVersion: 2, acknowledged: false, acknowledgedCount: 0 });
    expect(await db.prepare("SELECT state,version FROM staff_tasks WHERE id=?").bind(taskId)
      .first()).toMatchObject({ state: "in_progress", version: 1 });
  });

  it("hides the entire handover when a recipient loses source access", async () => {
    const view = await handoverView(db, owner, snapshot);
    const published = await publishHandover(db, owner, snapshot, {
      date: view.date, body: "Printer repair is waiting for parts.",
      sources: [{ id: taskId, sourceVersion: 1 }], recipientIds: [receiver.userId],
    }, key());
    await db.prepare("UPDATE staff_tasks SET assignee_user_id=?,version=version+1 WHERE id=?")
      .bind(outsider.userId, taskId).run();
    await expect(readablePost(db, receiver, published.id))
      .rejects.toMatchObject({ code: "POST_NOT_FOUND", status: 404 });
    await expect(acknowledgeHandover(db, receiver, published.id, key()))
      .rejects.toMatchObject({ code: "POST_NOT_FOUND", status: 404 });
    expect((await readablePost(db, owner, published.id)).id).toBe(published.id);
  });
});
