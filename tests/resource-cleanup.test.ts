import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { cleanupBetaResources } from "../src/server/beta-maintenance";

const db = env.H001_DB, files = env.H001_FILES, now = Date.parse("2026-10-05T00:00:00Z"), holding = "h001";
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
afterEach(() => vi.restoreAllMocks());
async function upload(id: string, expiresAt: number, status = "uploaded") {
  await db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,final_key,status,created_by,expires_at,created_at,updated_at) VALUES(?,'test','test','supporting','test.txt','text/plain',1,?,?,?,'test',?,?,?)").bind(id, `${holding}/staging/${id}`, `${holding}/objects/${id}/r1`, status, expiresAt, now - 200000, now - 200000).run();
  await files.put(`${holding}/staging/${id}`, "x"); await files.put(`${holding}/objects/${id}/r1`, "x");
}
it("deletes expired unbound upload bytes while retaining attachment-owned and unexpired bytes", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const id = crypto.randomUUID();
  await upload(`${id}-expired`, now - 60001);
  await upload(`${id}-current`, now - 60000);
  await upload(`${id}-attached`, now - 60001);
  await db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES(?,?,'test','test','supporting',?,1,'test.txt','text/plain',1,?,'test',?)").bind(`${id}-attachment`, `${id}-attached`, `${holding}/objects/${id}-attached/r1`, "a".repeat(64), now - 200000).run();
  await cleanupBetaResources(db, files, holding);
  expect(await files.head(`${holding}/objects/${id}-expired/r1`)).toBeNull();
  expect(await files.head(`${holding}/staging/${id}-expired`)).toBeNull();
  expect(await files.head(`${holding}/objects/${id}-current/r1`)).not.toBeNull();
  expect(await files.head(`${holding}/objects/${id}-attached/r1`)).not.toBeNull();
  expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(`${id}-expired`).first("status")).toBe("expired");
  expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(`${id}-attached`).first("status")).toBe("uploaded");
});
it("expires abandoned AI reservations and clears expired results without clearing current ready drafts", async () => {
  vi.spyOn(Date, "now").mockReturnValue(now);
  const id = crypto.randomUUID();
  await db.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at) VALUES(?,'actor','Actor','company','Synthetic cleanup prerequisite',?,?)").bind(`${id}-post`, now - 200000, now - 200000).run();
  for (const [suffix, state, createdAt, expiresAt] of [["old-pending", "pending", now - 120001, now + 1000], ["expired-ready", "ready", now - 1000, now - 1], ["current-ready", "ready", now - 1000, now + 1000]] as const)
    await db.prepare("INSERT INTO social_ai_drafts(id,post_id,actor_id,kind,source_version,source_hash,comment_ids_json,sources_json,result_json,state,model,created_at,expires_at) VALUES(?,?,'actor','summary',1,'hash','[]','[]','{}',?,'simulation',?,?)").bind(`${id}-${suffix}`, `${id}-post`, state, createdAt, expiresAt).run();
  await db.prepare("INSERT INTO social_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES(?,'company','actor','2026-10','reserved',?,?)").bind(`${id}-old-pending`, now - 1, now - 1000).run();
  await cleanupBetaResources(db, files, holding);
  for (const suffix of ["old-pending", "expired-ready"]) expect(await db.prepare("SELECT state,result_json FROM social_ai_drafts WHERE id=?").bind(`${id}-${suffix}`).first()).toMatchObject({ state: "failed", result_json: null });
  expect(await db.prepare("SELECT state,result_json FROM social_ai_drafts WHERE id=?").bind(`${id}-current-ready`).first()).toMatchObject({ state: "ready", result_json: "{}" });
  expect(await db.prepare("SELECT state FROM social_ai_usage WHERE id=?").bind(`${id}-old-pending`).first("state")).toBe("failed");
});
