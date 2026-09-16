import { beforeAll, beforeEach, expect, it } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { markHomeRead, unreadHomeCount } from "../src/server/navigation-counts";

let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { f = await expenseFixture(); });
async function post(createdAt: number, author = f.owner.userId, state = "published", company = f.company) {
  await db.prepare(`INSERT INTO social_posts(id,kind,author_id,author_name,company_id,body,state,created_at,updated_at)
    VALUES(?,'team',?,'Author',?,'Update',?,?,?)`)
    .bind(key(), author, company, state, createdAt, createdAt).run();
}

it("clears the entire feed, retains the cursor across sessions, and counts only subsequent posts", async () => {
  const now = Date.now();
  for (let i = 0; i < 25; i++) await post(now - 100 + i);
  await post(now - 50, f.claimant.userId);
  await post(now - 50, f.owner.userId, "moderated");
  expect(await unreadHomeCount(db, f.claimant, now)).toBe(25);
  await markHomeRead(db, f.claimant, now);
  expect(await unreadHomeCount(db, { ...f.claimant }, now)).toBe(0);
  await post(now + 1);
  expect(await unreadHomeCount(db, f.claimant, now + 2)).toBe(1);
  await markHomeRead(db, f.claimant, now - 1000);
  expect(await unreadHomeCount(db, f.claimant, now + 2)).toBe(1);
});

it("keeps read state separate for different users and workspace scopes", async () => {
  const now = Date.now();
  await post(now - 10);
  await markHomeRead(db, f.claimant, now);
  expect(await unreadHomeCount(db, f.head, now)).toBe(1);
  const branch = { ...f.claimant, scope: { ...f.claimant.scope, kind: "branch" as const, branchSlug: "other", branchId: f.branch, branchName: "Other" } };
  expect(await unreadHomeCount(db, branch, now)).toBe(1);
});
