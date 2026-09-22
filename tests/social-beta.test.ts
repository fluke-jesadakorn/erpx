import { sealSecret } from "../src/server/iam-mfa";
import { loadIamAuthority } from "../src/server/iam-store";
import { selectWorkRole } from "../src/server/work-role-store";
import { decideIamReview,validateIamGrant,grantInput } from "../src/server/iam-service";
import { iamActorFromWork } from "../src/server/iam-context";
import { drainSocialAgent, retryAgentReply } from "../src/server/social-agent";
import { textSuggestions } from "../src/server/social-text";
import type { Mention } from "../src/lib/social-text";
import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, beforeEach, describe, it, expect, vi } from "vitest";
import {
  onboard,
  memberStatements,
  branchStatements,
} from "../src/server/beta-onboarding";
import {
  postView,
  postComments,
  homePosts,
  savePostDraft,
  publishPost,
  postAction,
  socialUploadOwner,
} from "../src/server/social-home";
import { sourceSnapshot, readablePost } from "../src/server/social-source";
import { createAiDraft, aiResultSchema, callAi } from "../src/server/social-ai";
import {
  saveTask,
  taskSchema,
  taskDetail,
  transitionTask,
} from "../src/server/staff-tasks";
import { betaDestinationAllowed } from "../src/server/beta-access";
import {
  addBetaBranch,
  inviteMember,
  acceptMember,
} from "../src/server/beta-team";
import {
  resourceSlots,
  registeredDatabases,
  registeredResources,
} from "../src/lib/holding-registry";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({ runtimeEnv: async () => env, holdingResources: async () => ({write:env.H001_DB,read:env.H001_READ_DB,files:env.H001_FILES}) }));
const db = env.H001_DB,
  control = env.CONTROL_DB,
  key = () => crypto.randomUUID();
let company: string,
  branch: string,
  owner: WorkContext,
  worker: WorkContext,
  reviewer: WorkContext;
const registry = JSON.stringify([
  {
    id: "testbeta1",
    write: "TEST_ONE_WRITE",
    read: "TEST_ONE_READ",
    files: "TEST_ONE_FILES",
    beta: true,
  },
  {
    id: "testbeta2",
    write: "TEST_TWO_WRITE",
    read: "TEST_TWO_READ",
    files: "TEST_TWO_FILES",
    beta: true,
  },
]);
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
  await applyMigrations(
    env.H002_DB,
    env.STAFF_TEST_MIGRATIONS as TestMigration[],
  );
  await applyMigrations(
    control,
    env.CONTROL_TEST_MIGRATIONS as TestMigration[],
  );
});
async function freshIam(actor:WorkContext){
 const now=Date.now(),sessionId=key();
 await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?) ON CONFLICT DO NOTHING').bind(actor.userId,actor.name,actor.email,now,now).run();
 await control.prepare('INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now+86400000).toISOString(),key(),new Date().toISOString(),new Date().toISOString(),actor.userId).run();
 await control.prepare('INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true) ON CONFLICT DO NOTHING').bind(actor.userId,await sealSecret('JBSWY3DPEHPK3PXP')).run();
 await control.prepare('INSERT INTO iam_mfa_sessions VALUES(?,?,?)').bind(sessionId,actor.userId,now).run();
 await selectWorkRole(db,{sessionId,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,assignmentVersion:1,revision:0,expiresAt:now+86400000});
 actor.authority={sessionId,assignmentVersion:1,revision:1};actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);return actor;
}
async function approveInvitation(){
 const id=key(),assignmentId=key(),second={...owner,userId:id,name:'Independent reviewer',email:id+'@example.test',assignmentId,authority:undefined};
 await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,'holding_owner',?,'holding','[]',0,?)").bind(assignmentId,id,company,Date.now()+100*86400000).run();
 await freshIam(second);
 const pending=await db.prepare("SELECT id FROM iam_reviews WHERE company_id=? AND kind='beta.invitation' AND status='pending'").bind(company).first<{id:string}>();
 const actor=iamActorFromWork(second);await decideIamReview(db,actor,pending!.id,{expectedVersion:1,decision:'approve',reason:'Verified invitation role and expiry'},key(),async(c,kind,p)=>{expect(kind).toBe('beta.invitation');for(const grant of p.grants as unknown[])await validateIamGrant(c,actor,grantInput.parse(grant),String(p.parentId));});
}
beforeEach(async () => {
  company = `co-${key()}`;
  branch = `br-${key()}`;
  await db.batch([
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Test company',?)",
      )
      .bind(company, company, company),
    ...branchStatements(db, company, branch, "Branch", branch),
    db
      .prepare(
        "INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'member','Member',1,1,1)",
      )
      .bind(`level-${company}`, company),
    db
      .prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)")
      .bind(company),
  ]);
  const staff = async (name: string, role: string): Promise<WorkContext> => {
    const id = key();
    await db.batch(
      memberStatements(
        db,
        company,
        branch,
        { id, name, email: `${id}@example.test`, emailVerified: true },
        role === "holding_owner",
        null,
      ),
    );
    return {
      userId: id,
      name,
      email: `${id}@example.test`,
      assignmentId: `beta-assignment-${id}`,
      role,
      companyId: company,
      branchId: role === "holding_owner" ? null : branch,
      departmentId: null,
      holdingId: "h001",
      holdingSlug: "acme",
      holdingName: "Test",
      scope: {
        kind: "holding",
        holdingId: "h001",
        holdingSlug: "acme",
        holdingName: "Test",
        companyId: company,
      },
      assignmentScope: role === "holding_owner" ? "holding" : "branch",
      inherited: false,
      modules: [],
      assignments: [],
      workspaces: [],
    };
  };
  owner = await freshIam(await staff("Owner", "holding_owner"));
  worker = await staff("Worker", "employee");
  reviewer = await staff("Reviewer", "employee");
});
async function post() {
  const draft = await savePostDraft(
    db,
    owner,
    null,
    {
      body: "เครื่องปรับอากาศเสีย กรุณาช่วยตรวจสอบ",
      branchId: branch,
      expectedVersion: 1,
    },
    key(),
  );
  return publishPost(db, owner, draft.id, draft.version, key());
}
async function source(id: string) {
  const snap = await sourceSnapshot(db, owner, id, []);
  return {
    postId: id,
    sourceVersion: snap.post.version,
    sourceHash: snap.hash,
    commentIds: [],
    aiDraftId: null,
  };
}
function task() {
  return taskSchema.parse({
    title: "Repair air conditioner",
    description: "Inspect and attach evidence",
    assigneeUserId: worker.userId,
    dueAt: "2026-10-01T12:00:00+07:00",
    completionApprovers: [reviewer.userId],
  });
}
describe("social beta isolation and work", () => {
  it("keeps drafts private, then uses the normal independent review workflow", async () => {
    const draft = await savePostDraft(
      db,
      owner,
      null,
      { body: "Need help", branchId: branch, expectedVersion: 1 },
      key(),
    );
    await expect(socialUploadOwner(db, worker, draft.id)).rejects.toMatchObject(
      { status: 403 },
    );
    const p = await publishPost(db, owner, draft.id, 1, key());
    await expect(
      socialUploadOwner(db, worker, draft.id),
    ).resolves.toMatchObject({ published_post_id: p.id });
    const created = await saveTask(
      db,
      owner,
      null,
      task(),
      key(),
      false,
      await source(p.id),
    );
    const id = String(created.id);
    expect((await taskDetail(db, owner, id)).task.state).toBe("draft");
    await transitionTask(
      db,
      owner,
      id,
      { action: "submit", expectedVersion: 1, note: "" },
      key(),
    );
    await transitionTask(
      db,
      worker,
      id,
      { action: "start", expectedVersion: 2, note: "" },
      key(),
    );
    await transitionTask(
      db,
      worker,
      id,
      {
        action: "complete",
        expectedVersion: 3,
        note: "Replaced the faulty switch. Tested.",
      },
      key(),
    );
    await expect(
      transitionTask(
        db,
        worker,
        id,
        { action: "approve", expectedVersion: 4, note: "My own work" },
        key(),
      ),
    ).rejects.toMatchObject({ status: 403 });
    await transitionTask(
      db,
      reviewer,
      id,
      { action: "approve", expectedVersion: 4, note: "Inspected and accepted" },
      key(),
    );
    expect((await taskDetail(db, owner, id)).task.state).toBe("completed");
    expect(
      await db
        .prepare("SELECT count(*) n FROM social_task_origins WHERE post_id=?")
        .bind(p.id)
        .first("n"),
    ).toBe(1);
  });
  it("commits exactly one task, source, receipt and assignment notification during a race", async () => {
    const p = await post(),
      s = await source(p.id);
    const outcomes = await Promise.allSettled([
      saveTask(db, owner, null, task(), key(), false, s),
      saveTask(db, owner, null, task(), key(), false, s),
    ]);
    expect(outcomes.filter((x) => x.status === "fulfilled")).toHaveLength(1);
    const id = `task-post-${p.id}`;
    expect(
      await db
        .prepare("SELECT count(*) n FROM staff_tasks WHERE id=?")
        .bind(id)
        .first("n"),
    ).toBe(1);
    expect(
      await db
        .prepare(
          "SELECT count(*) n FROM audit_events WHERE record_id=? AND action='task.create'",
        )
        .bind(id)
        .first("n"),
    ).toBe(1);
    expect(
      await db
        .prepare(
          "SELECT count(*) n FROM outbox_events WHERE aggregate_id=? AND event_type='task.create'",
        )
        .bind(id)
        .first("n"),
    ).toBe(1);
  });
  it("rechecks source version and permissions before any task writes", async () => {
    const p = await post(),
      s = await source(p.id);
    await postAction(
      db,
      worker,
      p.id,
      { action: "comment", body: "New information" },
      key(),
    );
    await expect(
      saveTask(db, owner, null, task(), key(), false, s),
    ).rejects.toMatchObject({ code: "SOURCE_CHANGED" });
    expect(
      await db
        .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
        .bind(p.id)
        .first(),
    ).toBeNull();
    await db
      .prepare("UPDATE assignments SET revoked_at=1 WHERE user_id=?")
      .bind(worker.userId)
      .run();
    await expect(
      saveTask(db, owner, null, task(), key(), false, await source(p.id)),
    ).rejects.toMatchObject({ status: 404 });
  });
  it("denies another company, another branch, revoked accounts and hidden source files", async () => {
    const p = await post();
    await expect(
      readablePost(db, { ...worker, companyId: "another-company" }, p.id),
    ).rejects.toMatchObject({ status: 404 });
    await expect(readablePost(env.H002_DB, worker, p.id)).rejects.toMatchObject(
      { status: 404 },
    );
    await db
      .prepare(
        "UPDATE assignments SET scope_node_id='other-branch' WHERE user_id=?",
      )
      .bind(worker.userId)
      .run();
    await expect(readablePost(db, worker, p.id)).rejects.toMatchObject({
      status: 404,
    });
    await postAction(db, owner, p.id, { action: "hide" }, key());
    await expect(readablePost(db, owner, p.id)).rejects.toMatchObject({
      status: 404,
    });
  });
  it("rejects idempotency keys reused by another actor and leaves no extra posts", async () => {
    const k = key(),
      input = { body: "Private", branchId: branch, expectedVersion: 1 };
    await savePostDraft(db, owner, null, input, k);
    await expect(
      savePostDraft(db, worker, null, input, k),
    ).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
  });
  it("keeps beta modules unavailable at server destinations", () => {
    for (const path of [
      "finance",
      "people/payroll",
      "modules",
      "records/search",
      "posts",
      "posts/a/comments",
    ])
      expect(betaDestinationAllowed(path, true)).toBe(false);
    expect(betaDestinationAllowed("posts/a/task", true)).toBe(true);
    expect(betaDestinationAllowed("staff-tasks/a/transition", true)).toBe(true);
    expect(betaDestinationAllowed("people", false)).toBe(true);
    expect(betaDestinationAllowed("staff/levels", true)).toBe(true);
    expect(betaDestinationAllowed("ai-providers", true)).toBe(true);
    expect(betaDestinationAllowed("expenses", true)).toBe(true);
  });
});
describe("AI reservations and private output", () => {
  const output = (postId: string) => ({
    result: {
      summary: "Air conditioning needs inspection.",
      title: null,
      description: null,
      assigneeUserId: null,
      reviewerUserId: null,
      dueAt: null,
      missing: ["Assignee", "Due date"],
      citations: [postId],
    },
    inputTokens: 10,
    outputTokens: 5,
  });
  it("reserves before calling, validates references and persists only a private draft", async () => {
    const p = await post();
    const provider = vi.fn(async () => output(p.id));
    const draft = await createAiDraft(
      db,
      owner,
      p.id,
      { kind: "task", commentIds: [] },
      { AI_MODEL: "test" },
      provider,
    );
    expect(draft.result.assigneeUserId).toBeNull();
    expect(provider).toHaveBeenCalledOnce();
    expect(
      await db
        .prepare("SELECT actor_id FROM social_ai_drafts WHERE id=?")
        .bind(draft.id)
        .first("actor_id"),
    ).toBe(owner.userId);
    expect(
      await db
        .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
        .bind(p.id)
        .first(),
    ).toBeNull();
  });
  it("atomically limits concurrent requests and monthly quota", async () => {
    const p = await post();
    await db
      .prepare("UPDATE beta_policy SET ai_monthly_limit=1 WHERE company_id=?")
      .bind(company)
      .run();
    const provider = vi.fn(async () => output(p.id));
    const results = await Promise.allSettled([
      createAiDraft(
        db,
        owner,
        p.id,
        { kind: "task", commentIds: [] },
        { AI_MODEL: "test" },
        provider,
      ),
      createAiDraft(
        db,
        owner,
        p.id,
        { kind: "task", commentIds: [] },
        { AI_MODEL: "test" },
        provider,
      ),
    ]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    expect(provider).toHaveBeenCalledOnce();
  });
  it.each(["timeout", "schema", "citation", "revocation", "changed"] as const)(
    "fails safely for %s without creating work",
    async (failure) => {
      const p = await post();
      const provider = async () => {
        if (failure === "timeout") throw new Error("Timeout");
        if (failure === "revocation")
          await db
            .prepare("UPDATE assignments SET revoked_at=1 WHERE id=?")
            .bind(owner.assignmentId)
            .run();
        if (failure === "changed")
          await postAction(
            db,
            worker,
            p.id,
            { action: "comment", body: "Changed" },
            key(),
          );
        const result = output(p.id);
        if (failure === "schema") result.result.summary = "x".repeat(3001);
        if (failure === "citation") result.result.citations = ["foreign-post"];
        return result;
      };
      await expect(
        createAiDraft(
          db,
          owner,
          p.id,
          { kind: "task", commentIds: [] },
          { AI_MODEL: "test" },
          provider,
        ),
      ).rejects.toMatchObject({ code: "AI_UNAVAILABLE" });
      expect(
        await db
          .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
          .bind(p.id)
          .first(),
      ).toBeNull();
      expect(
        await db
          .prepare("SELECT state FROM social_ai_usage WHERE company_id=?")
          .bind(company)
          .first("state"),
      ).toBe("failed");
    },
  );
  it("handles provider refusal, invalid JSON and excessive output", async () => {
    const original = globalThis.fetch;
    try {
      for (const payload of [
        {
          choices: [{ finish_reason: "stop", message: { refusal: "Refused" } }],
        },
        { choices: [{ finish_reason: "length", message: { content: "{}" } }] },
        {
          choices: [
            { finish_reason: "stop", message: { content: "not json" } },
          ],
        },
      ]) {
        globalThis.fetch = vi.fn(
          async () => new Response(JSON.stringify(payload), { status: 200 }),
        );
        await expect(
          callAi(
            {
              AI_BASE_URL: "https://provider.example/v1",
              AI_API_KEY: "test",
              AI_MODEL: "test",
            },
            {},
          ),
        ).rejects.toThrow();
      }
    } finally {
      globalThis.fetch = original;
    }
  });
  it("requires explicit missing task fields in the schema", () => {
    expect(aiResultSchema.safeParse({ summary: "Incomplete" }).success).toBe(
      false,
    );
  });
});
describe("tenant admission and verified invitations", () => {
  it("resumes a claim and admits only available isolated slots", async () => {
    await applyMigrations(
      env.H001_READ_DB,
      env.STAFF_TEST_MIGRATIONS as TestMigration[],
    );
    await applyMigrations(
      env.H002_READ_DB,
      env.STAFF_TEST_MIGRATIONS as TestMigration[],
    );
    const betaEnv = {
      ...env,
      TEST_ONE_WRITE: env.H001_READ_DB,
      TEST_ONE_READ: env.H001_DB,
      TEST_ONE_FILES: env.H001_FILES,
      TEST_TWO_WRITE: env.H002_READ_DB,
      TEST_TWO_READ: env.H002_DB,
      TEST_TWO_FILES: env.H002_FILES,
      ERPX_RESOURCE_SLOTS: registry,
      ERPX_PUBLIC_BETA: "true",
    };
    const users = await Promise.all(
      [1, 2, 3].map(async (n) => {
        const u = {
          id: `onboard-${key()}`,
          name: `Owner ${n}`,
          email: `${key()}@example.test`,
          emailVerified: true,
        };
        await control
          .prepare(
            "INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,1,1)",
          )
          .bind(u.id, u.name, u.email)
          .run();
        return u;
      }),
    );
    await control.batch(
      ["testbeta1", "testbeta2"].map((id) =>
        control
          .prepare(
            "INSERT INTO beta_slots(holding_id,created_at,updated_at) VALUES(?,1,1)",
          )
          .bind(id),
      ),
    );
    const inputs = users.map((_, i) => ({
      companyName: `Company ${i}`,
      branchName: "First branch",
      slug: `company-${key().slice(0, 8)}`,
    }));
    await expect(
      onboard(
        {
          ...betaEnv,
          TEST_ONE_WRITE: {
            prepare: env.H001_READ_DB.prepare.bind(env.H001_READ_DB),
            batch: async () => {
              throw new Error("Injected provisioning failure");
            },
          },
        },
        users[0],
        inputs[0],
      ),
    ).rejects.toThrow("Injected provisioning failure");
    expect(
      await control
        .prepare("SELECT state FROM beta_slots WHERE owner_user_id=?")
        .bind(users[0].id)
        .first("state"),
    ).toBe("provisioning");
    const results = await Promise.all(
      users.map((u, i) => onboard(betaEnv, u, inputs[i])),
    );
    expect(results.filter((x) => x.state === "active")).toHaveLength(2);
    expect(results.filter((x) => x.state === "waitlisted")).toHaveLength(1);
    const activeSlots = (
      await control
        .prepare("SELECT holding_id,owner_user_id FROM beta_slots WHERE state='active' ORDER BY holding_id")
        .all<{ holding_id: string; owner_user_id: string }>()
    ).results;
    for (const activeSlot of activeSlots) {
      const holding = registeredResources(betaEnv, activeSlot.holding_id).write;
      const companyId = `company-${activeSlot.holding_id}`;
      expect(
        await holding
          .prepare("SELECT status,version FROM company_provisioning WHERE company_id=? AND holding_id=?")
          .bind(companyId, activeSlot.holding_id)
          .first(),
      ).toEqual({ status: "active", version: 2 });
      expect(
        await holding
          .prepare("SELECT count(*) count FROM staff_levels WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6")
          .bind(companyId)
          .first("count"),
      ).toBe(6);
      expect(
        await holding
          .prepare("SELECT count(*) count FROM organization_positions WHERE company_id=? AND active=1 AND is_ceo=1")
          .bind(companyId)
          .first("count"),
      ).toBe(1);
      expect(
        await holding
          .prepare("SELECT level_id,position_id,job_title FROM hr_engagements WHERE company_id=? AND id=?")
          .bind(companyId, `engagement-${activeSlot.owner_user_id}`)
          .first(),
      ).toMatchObject({
        level_id: `org-level-${companyId}-1`,
        position_id: `company-ceo-${companyId}`,
        job_title: "Chief Executive Officer",
      });
      expect(
        (
          await holding
            .prepare("SELECT action FROM company_provisioning_events WHERE company_id=? ORDER BY created_at,id")
            .bind(companyId)
            .all<{ action: string }>()
        ).results.map((event) => event.action),
      ).toEqual(["started", "completed"]);
    }
    const accepted = results.findIndex((x) => x.state === "active");
    await expect(
      onboard(betaEnv, users[accepted], inputs[accepted]),
    ).resolves.toMatchObject(results[accepted]);
    expect(
      await control
        .prepare("SELECT count(*) n FROM beta_slots WHERE state='active'")
        .first("n"),
    ).toBe(2);
    await expect(
      onboard(betaEnv, { ...users[0], emailVerified: false }, inputs[0]),
    ).rejects.toMatchObject({ code: "VERIFY_EMAIL" });
  });
  it("links only the invited verified account and retries without duplicate access", async () => {
    const user = {
        id: key(),
        name: "Invited person",
        email: `${key()}@example.test`,
        emailVerified: true,
      },
      token = key();
    await inviteMember(
      db,
      owner,
      { email: user.email, branchId: branch },
      key(),
      token,
    );
    await expect(
      acceptMember(db, { ...user, email: "someone-else@example.test" }, token),
    ).rejects.toMatchObject({ status: 404 });
    await expect(
      acceptMember(db, { ...user, emailVerified: false }, token),
    ).rejects.toMatchObject({ code: "VERIFY_EMAIL" });
    await expect(acceptMember(db,user,token)).rejects.toMatchObject({code:'INVITATION_REVIEW_REQUIRED'});
    await approveInvitation();
    await acceptMember(db, user, token);
    await acceptMember(db, user, token);
    expect(
      await db
        .prepare("SELECT count(*) n FROM assignments WHERE user_id=?")
        .bind(user.id)
        .first("n"),
    ).toBe(1);
  });
  it("validates registry capacity and refuses arbitrary binding names", () => {
    expect(resourceSlots({ ERPX_RESOURCE_SLOTS: registry })).toHaveLength(2);
    expect(() => registeredDatabases(env, "unexpected")).toThrow(
      "HOLDING_NOT_FOUND",
    );
    expect(() =>
      resourceSlots({
        ERPX_RESOURCE_SLOTS: JSON.stringify(
          Array.from({ length: 21 }, (_, i) => ({
            id: `beta${i}`,
            write: `W${i}_DB`,
            read: `R${i}_DB`,
            files: `F${i}_FILES`,
            beta: true,
          })),
        ),
      }),
    ).toThrow("BETA_CAPACITY_EXCEEDED");
  });
  it("accepts an owner-issued IT invitation alongside ordinary employee access", async () => {
    const user = {
        id: key(),
        name: "Invited IT administrator",
        email: `${key()}@example.test`,
        emailVerified: true,
      },
      token = key();
    await inviteMember(
      db,
      owner,
      { email: user.email, branchId: branch, role: "it_admin" },
      key(),
      token,
    );
    await expect(acceptMember(db,user,token)).rejects.toMatchObject({code:'INVITATION_REVIEW_REQUIRED'});
    await approveInvitation();
    await acceptMember(db, user, token);
    await acceptMember(db, user, token);
    const assignments = (
      await db
        .prepare(
          "SELECT role,scope_kind,company_id FROM assignments WHERE user_id=? ORDER BY role",
        )
        .bind(user.id)
        .all()
    ).results;
    expect(assignments).toHaveLength(2);
    expect(assignments.find((a) => a.role === "it_admin")).toEqual({
      role: "it_admin",
      scope_kind: "holding",
      company_id: company,
    });
    expect(assignments.some((a) => a.role === "employee")).toBe(true);
  });
});

it("paginates tied timestamps without dropping or duplicating conversations", async () => {
  const timestamp = Date.now() - 100;
  await db.batch(
    Array.from({ length: 22 }, (_, i) =>
      db
        .prepare(
          "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,body,last_command_id,created_at,updated_at) VALUES(?,'team',?,?,?,'Update',?,?,?)",
        )
        .bind(
          `paging-${company}-${String(i).padStart(2, "0")}`,
          owner.userId,
          owner.name,
          company,
          key(),
          timestamp,
          timestamp,
        ),
    ),
  );
  const first = await homePosts(db, owner),
    last = first.at(-1)!;
  const next = await homePosts(db, owner, {
    before: last.created_at,
    beforeId: last.id,
  });
  expect(first).toHaveLength(20);
  expect(next).toHaveLength(2);
  expect(new Set([...first, ...next].map((p) => p.id)).size).toBe(22);
});
it("enforces member and branch capacity before granting access", async () => {
  await db
    .prepare(
      "UPDATE beta_policy SET member_limit=3,branch_limit=1 WHERE company_id=?",
    )
    .bind(company)
    .run();
  await expect(
    inviteMember(
      db,
      owner,
      { email: "full@example.test", branchId: branch },
      key(),
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
  await expect(
    addBetaBranch(db, owner, "Extra branch", key()),
  ).rejects.toMatchObject({ status: 409 });
});

it("lets the original reporter track status without granting task or evidence access", async () => {
  const p = await post();
  await saveTask(db, worker, null, task(), key(), false, await source(p.id));
  const detail = await postView(db, owner, p.id);
  expect(detail.trackingState).toBe("draft");
  expect(detail.task).toBeNull();
  await expect(
    taskDetail(db, owner, `task-post-${p.id}`),
  ).rejects.toMatchObject({ status: 404 });
});

const assistantMention: Mention = {kind:"agent",targetId:"erpx-assistant",label:"ERPX assistant",start:0,end:15};
const replyEnv={...env,AI_BASE_URL:"https://provider.example/v1",AI_API_KEY:"test",AI_MODEL:"test"};
async function assistantPost() {
 const draft=await savePostDraft(db,owner,null,{body:"@ERPX assistant Help #ทีมงาน",mentions:[assistantMention],branchId:branch,expectedVersion:1},key());
 expect(await db.prepare("SELECT count(*) n FROM social_agent_replies WHERE owner_id=?").bind(draft.id).first<number>("n")).toBe(0);
 const p=await publishPost(db,owner,draft.id,draft.version,key());
 return {...p,draft};
}
describe("inline mentions, hashtags and assistant replies",()=>{
 it("preserves draft metadata, indexes hashtags and queues only once",async()=>{
  const p=await assistantPost();
  await publishPost(db,owner,p.draft.id,p.draft.version,key());
  const view=await postView(db,owner,p.id);
  expect(view.mentions).toEqual([assistantMention]);expect(view.agentReplies).toHaveLength(1);
  expect((await homePosts(db,owner,{tag:"ทีมงาน"})).map(p=>p.id)).toContain(p.id);
  expect(await textSuggestions(db,owner,"#","ทีม")).toContainEqual({kind:"tag",targetId:"ทีมงาน",label:"ทีมงาน",detail:"Hashtag"});
  await postAction(db,owner,p.id,{action:"comment",body:"Another #release"},key());
  expect((await homePosts(db,owner,{tag:"release"})).map(p=>p.id)).toContain(p.id);
  await postAction(db,owner,p.id,{action:"hide"},key());
  expect(await textSuggestions(db,owner,"#","ทีม")).toEqual([]);
 });
 it("rejects forged targets and stale offsets",async()=>{
  await expect(savePostDraft(db,owner,null,{body:"@ERPX assistant",mentions:[{...assistantMention,targetId:"fake"}],branchId:branch,expectedVersion:1},key())).rejects.toMatchObject({code:"MENTION_UNAVAILABLE"});
  await expect(savePostDraft(db,owner,null,{body:"edited",mentions:[assistantMention],branchId:branch,expectedVersion:1},key())).rejects.toMatchObject({code:"MENTION_CHANGED"});
 });
 it("publishes one attributed AI reply, accounts usage, and never recursively queues",async()=>{
  const p=await assistantPost();
  const provider=vi.fn(async()=>({result:{body:"Here is some help. @ERPX assistant"},inputTokens:10,outputTokens:8}));
  await Promise.all([drainSocialAgent(db,replyEnv,p.id,provider),drainSocialAgent(db,replyEnv,p.id,provider)]);
  await drainSocialAgent(db,replyEnv,p.id,provider);
  const view=await postView(db,owner,p.id);
  expect(provider).toHaveBeenCalledTimes(1);expect(view.comments).toHaveLength(1);
  expect(view.comments[0].author_kind).toBe("agent");expect(view.agentReplies[0].state).toBe("completed");
  expect(await db.prepare("SELECT output_tokens FROM social_ai_usage WHERE id=?").bind(`${view.agentReplies[0].id}:1`).first<number>("output_tokens")).toBe(8);
 });
 it("keeps the post when AI fails and lets only its requester retry",async()=>{
  const p=await assistantPost();
  await drainSocialAgent(db,replyEnv,p.id,async()=>{throw new Error("offline");});
  let view=await postView(db,owner,p.id);expect(view.agentReplies[0].state).toBe("failed");
  await expect(retryAgentReply(db,worker,view.agentReplies[0].id,key())).rejects.toMatchObject({code:"REPLY_UNAVAILABLE"});
  const k=key();await retryAgentReply(db,owner,view.agentReplies[0].id,k);await retryAgentReply(db,owner,view.agentReplies[0].id,k);
  await drainSocialAgent(db,replyEnv,p.id,async()=>({result:{body:"Ready"},inputTokens:1,outputTokens:1}));
  view=await postView(db,owner,p.id);expect(view.comments).toHaveLength(1);expect(view.agentReplies[0].state).toBe("completed");
 });
 it("blocks exhausted quotas and revoked access without publishing",async()=>{
  const p=await assistantPost();const provider=vi.fn(async()=>({result:{body:"Ready"},inputTokens:1,outputTokens:1}));
  await db.prepare("UPDATE beta_policy SET ai_monthly_limit=0 WHERE company_id=?").bind(company).run();
  await drainSocialAgent(db,replyEnv,p.id,provider);expect(provider).not.toHaveBeenCalled();
  const view=await postView(db,owner,p.id);expect(view.agentReplies[0].state).toBe("failed");
  await db.prepare("UPDATE beta_policy SET ai_monthly_limit=100 WHERE company_id=?").bind(company).run();
  await retryAgentReply(db,owner,view.agentReplies[0].id,key());
  await drainSocialAgent(db,replyEnv,p.id,async()=>{await db.prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?").bind(Date.now(),owner.userId).run();return {result:{body:"Do not publish"},inputTokens:1,outputTokens:1};});
  expect((await postView(db,worker,p.id)).comments).toHaveLength(0);
 });
 it("invokes the assistant from a comment and fails expired leases visibly",async()=>{
  const p=await post();await postAction(db,owner,p.id,{action:"comment",body:"@ERPX assistant Help",mentions:[assistantMention]},key());
  const view=await postView(db,owner,p.id);expect(view.agentReplies).toHaveLength(1);
  await db.prepare("UPDATE social_agent_replies SET state='running',lease_until=1 WHERE post_id=?").bind(p.id).run();
  const provider=vi.fn();await drainSocialAgent(db,replyEnv,p.id,provider);expect(provider).not.toHaveBeenCalled();
  expect((await postView(db,owner,p.id)).agentReplies[0].state).toBe("failed");
 });
});

it("scopes people/record suggestions and notifies only readable mentioned people",async()=>{
 for(const actor of [owner,worker,reviewer]) await control.batch([
  control.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,1,1) ON CONFLICT(id) DO NOTHING').bind(actor.userId,actor.name,actor.email),
  control.prepare("INSERT INTO principals(id,user_id,display_name,created_at) VALUES(?,?,?,1)").bind(actor.userId,actor.userId,actor.name),
 ]);
 // Reviewer belongs to another branch and must not receive a branch-post notification.
 const other=`other-${key()}`;await db.batch(branchStatements(db,company,other,"Other",other));
 await db.prepare("UPDATE assignments SET scope_node_id=? WHERE user_id=?").bind(other,reviewer.userId).run();
 owner.modules=["brand"];
 const rec=`rec-${key()}`;
 await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,fields_json,company_id,branch_id,created_by,updated_by,created_at,updated_at) VALUES('brand',?,'drafts','Launch','[]',?,?,?,?,1,1)").bind(rec,company,branch,owner.userId,owner.userId).run();
 const suggestions=await textSuggestions(db,owner,"@","");
 expect(suggestions.some(s=>s.targetId===worker.userId)).toBe(true);
 expect(suggestions.some(s=>s.targetId===`brand/${rec}`)).toBe(true);
 const body="@Worker @Reviewer @Launch";
 const mentions:Mention[]=[{kind:"person",targetId:worker.userId,label:"Worker",start:0,end:7},{kind:"person",targetId:reviewer.userId,label:"Reviewer",start:8,end:17},{kind:"record",targetId:`brand/${rec}`,label:"Launch",start:18,end:25}];
 const d=await savePostDraft(db,owner,null,{body,mentions,branchId:branch,expectedVersion:1},key());
 const p=await publishPost(db,owner,d.id,d.version,key());
 const notified=(await db.prepare("SELECT user_id FROM staff_notifications WHERE record_id=?").bind(p.id).all<{user_id:string}>()).results;
 expect(notified.map(n=>n.user_id)).toEqual([worker.userId]);
 await expect(savePostDraft(db,{...owner,modules:[]},null,{body,mentions,branchId:branch,expectedVersion:1},key())).rejects.toMatchObject({code:"MODULE_FORBIDDEN"});
 await db.prepare("UPDATE assignments SET revoked_at=1 WHERE user_id=?").bind(worker.userId).run();
 expect((await textSuggestions(db,owner,"@","Worker")).some(s=>s.kind==="person")).toBe(false);
});

it("keeps the newest replies visible in long conversations",async()=>{
 const p=await post(),now=Date.now();
 await db.batch(Array.from({length:55},(_,i)=>db.prepare("INSERT INTO social_comments(id,post_id,author_id,author_name,body,created_at,updated_at) VALUES(?,?,?,'Owner',?,?,?)").bind(`${p.id}-${i}`,p.id,owner.userId,`Reply ${i}`,now+i,now+i)));
 const view=await postView(db,owner,p.id);
 expect(view.comments).toHaveLength(20);expect(view.comments[0].body).toBe("Reply 35");expect(view.comments.at(-1)?.body).toBe("Reply 54");
 expect(view.commentCount).toBe(55);
 const ids:string[]=[];let cursor:string|undefined;
 do {const page=await postComments(db,owner,p.id,cursor);ids.push(...page.items.map(c=>c.id));cursor=page.nextCursor??undefined;} while(cursor);
 expect(ids).toHaveLength(55);expect(new Set(ids).size).toBe(55);
 const feed=(await homePosts(db,owner)).find(row=>row.id===p.id)!;
 expect(feed.comments).toHaveLength(2);expect(feed.commentCount).toBe(55);
 await expect(postComments(db,{...owner,companyId:'another-company'},p.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});
});
