import { randomUUID } from "node:crypto";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { memberProfileView, saveMemberProfile } from "../src/server/member-profiles";

vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
}));

beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
});

let first: Awaited<ReturnType<typeof seedExpenseCompany>>;
let second: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeEach(async () => {
  first = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  second = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  for (const owner of [first.owner, second.owner]) {
    await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
      .bind(owner.userId, owner.name, `${owner.userId}@example.test`, Date.now(), Date.now()).run();
  }
});

it("shows a colleague in the same Holding without exposing private HR data or another Company's posts", async () => {
  await env.H001_DB.prepare("UPDATE hr_people SET email=?,phone=? WHERE user_id=?")
    .bind("private@example.test", "private phone", second.owner.userId).run();
  await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
    .bind(randomUUID(), second.owner.userId, second.owner.name, second.company, "Private company update", Date.now(), Date.now()).run();
  const profile = await memberProfileView(first.owner, second.owner.userId);
  expect(profile.name).toBe(second.owner.name);
  expect(profile.own).toBe(false);
  expect(profile.posts).toEqual([]);
  expect(JSON.stringify(profile)).not.toMatch(/private@example\.test|private phone|Private company update/);
});

it("hides a profile immediately after its last Holding assignment is revoked", async () => {
  await env.H001_DB.prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?")
    .bind(Date.now(), second.owner.userId).run();
  await expect(memberProfileView(first.owner, second.owner.userId))
    .rejects.toMatchObject({ status: 404, code: "PROFILE_NOT_FOUND" });
});

it("shows only posts already readable to the viewer within the selected Company", async () => {
  const author = first.claimant.userId;
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(author, "Claimant", `${author}@example.test`, Date.now(), Date.now()).run();
  const visible = randomUUID(), restricted = randomUUID(), now = Date.now();
  await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,audience_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
    .bind(visible,author,"Claimant",first.company,"A company update","company",now,now).run();
  await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,audience_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
    .bind(restricted,author,"Claimant",first.company,"A private update","participants",now+1,now+1).run();
  await env.H001_DB.prepare("INSERT INTO social_post_participants(post_id,user_id) VALUES(?,?)")
    .bind(restricted,author).run();
  const profile = await memberProfileView(first.owner,author);
  expect(profile.posts.map(post => post.id)).toEqual([visible]);
  expect(JSON.stringify(profile)).not.toContain("A private update");
});

it("paginates posts with equal timestamps without skipping them", async () => {
  const author = first.claimant.userId;
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(author, "Claimant", `${author}@example.test`, Date.now(), Date.now()).run();
  const at = Date.now();
  const ids = Array.from({ length: 10 }, () => randomUUID());
  for (const id of ids) await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
    .bind(id,author,"Claimant",first.company,"A company update",at,at).run();
  const firstPage = await memberProfileView(first.owner,author);
  const secondPage = await memberProfileView(first.owner,author,firstPage.nextBefore!,firstPage.nextBeforeId!);
  expect(firstPage.posts).toHaveLength(8);
  expect(secondPage.posts).toHaveLength(2);
  expect(new Set([...firstPage.posts,...secondPage.posts].map(post => post.id))).toEqual(new Set(ids));
});

it("saves only the owner's profile with optimistic versioning and idempotency", async () => {
  const input = { bio: "Building our operations", links: [{ label: "Website", url: "https://example.test" }], version: 0 };
  const commandId = randomUUID();
  const saved = await saveMemberProfile(first.owner, input, commandId);
  expect(saved.version).toBe(1);
  expect(await saveMemberProfile(first.owner, input, commandId)).toEqual(saved);
  await expect(saveMemberProfile(first.owner, { ...input, bio: "Stale edit" }, randomUUID()))
    .rejects.toBeDefined();
  const profile = await memberProfileView(first.owner, first.owner.userId);
  expect(profile).toMatchObject({ bio: "Building our operations", links: input.links, version: 1, own: true });
  const colleague = await memberProfileView(first.owner, second.owner.userId);
  expect(colleague.bio).toBe("");
});
