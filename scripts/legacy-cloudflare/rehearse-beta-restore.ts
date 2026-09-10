import { getPlatformProxy } from "wrangler";
import { mkdir } from "node:fs/promises";
const run = `restore-${Date.now()}`,
  dir = `.scratch/${run}`;
await mkdir(dir, { recursive: true });
function wrangler(args: string[]) {
  const r = Bun.spawnSync(["bunx", "wrangler", ...args], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (r.exitCode)
    throw new Error(
      `Restore command failed: ${new TextDecoder().decode(r.stderr)}`,
    );
}
const config = {
  name: "erpx-restore-rehearsal",
  compatibility_date: "2026-09-05",
  d1_databases: [
    {
      binding: "RESTORE_DB",
      database_name: run,
      database_id: crypto.randomUUID(),
      migrations_dir: "../../migrations/holding-write",
    },
  ],
  r2_buckets: [{ binding: "RESTORE_FILES", bucket_name: run }],
};
const persistPath = `${process.cwd()}/${dir}/state`;
const path = `${dir}/wrangler.jsonc`;
await Bun.write(path, JSON.stringify(config));
wrangler([
  "d1",
  "migrations",
  "apply",
  "RESTORE_DB",
  "--config",
  path,
  "--persist-to",
  persistPath,
  "--local",
]);
wrangler([
  "d1",
  "export",
  "H001_DB",
  "--local",
  "--no-schema",
  "--output",
  `${dir}/holding.sql`,
]);
const original = await getPlatformProxy<
  Pick<CloudflareEnv, "H001_DB" | "H001_FILES">
>({ configPath: "wrangler.jsonc", persist: true, remoteBindings: false });
let data = (await Bun.file(`${dir}/holding.sql`).text())
  .split("\n")
  .filter(
    (line) =>
      !line.startsWith('INSERT INTO \"d1_migrations\"') &&
      !line.startsWith('INSERT INTO \"sqlite_sequence\"'),
  )
  .join("\n");
const triggers = await original.env.H001_DB.prepare(
  "SELECT name,sql FROM sqlite_schema WHERE type='trigger'",
).all<{ name: string; sql: string }>();
const quote = (name: string) => '"' + name.replaceAll('"', '""') + '"';
data =
  "PRAGMA defer_foreign_keys=TRUE;\n" +
  triggers.results
    .map((t) => `DROP TRIGGER IF EXISTS ${quote(t.name)};`)
    .join("\n") +
  "\n" +
  data +
  "\n" +
  triggers.results.map((t) => t.sql + ";").join("\n");
await Bun.write(`${dir}/holding.sql`, data);
wrangler([
  "d1",
  "execute",
  "RESTORE_DB",
  "--config",
  path,
  "--persist-to",
  persistPath,
  "--local",
  "--file",
  `${dir}/holding.sql`,
]);
const restored = await getPlatformProxy<{
  RESTORE_DB: D1Database;
  RESTORE_FILES: R2Bucket;
}>({
  configPath: path,
  persist: { path: `${persistPath}/v3` },
  remoteBindings: false,
});
try {
  const tables = [
    "assignments",
    "hr_people",
    "hr_engagements",
    "social_posts",
    "social_comments",
    "social_task_origins",
    "social_ai_drafts",
    "staff_tasks",
    "approval_instances",
    "audit_events",
    "outbox_events",
    "attachments",
  ];
  const counts: Record<string, number> = {};
  for (const table of tables) {
    const source = await original.env.H001_DB.prepare(
        `SELECT count(*) n FROM ${table}`,
      ).first<number>("n"),
      copy = await restored.env.RESTORE_DB.prepare(
        `SELECT count(*) n FROM ${table}`,
      ).first<number>("n");
    if (source !== copy) throw new Error(`Restore count mismatch: ${table}`);
    counts[table] = copy ?? 0;
    const sourceRows = await original.env.H001_DB.prepare(
      `SELECT * FROM ${table} ORDER BY rowid`,
    ).all();
    const copyRows = await restored.env.RESTORE_DB.prepare(
      `SELECT * FROM ${table} ORDER BY rowid`,
    ).all();
    const canonical = (rows: unknown[]) =>
      JSON.stringify(
        rows.map((row) =>
          Object.fromEntries(
            Object.entries(row as Record<string, unknown>).sort(([a], [b]) =>
              a.localeCompare(b),
            ),
          ),
        ),
      );
    if (canonical(sourceRows.results) !== canonical(copyRows.results))
      throw new Error(`Restored contents differ: ${table}`);
  }
  const bad = await restored.env.RESTORE_DB.prepare(
    "PRAGMA foreign_key_check",
  ).all();
  if (bad.results.length) throw new Error("Restored foreign key check failed");
  const files = await original.env.H001_DB.prepare(
    "SELECT object_key,checksum_sha256 FROM attachments WHERE revoked_at IS NULL",
  ).all<{ object_key: string; checksum_sha256: string }>();
  let verified = 0;
  for (const file of files.results) {
    const source = await original.env.H001_FILES.get(file.object_key);
    if (!source) throw new Error("Source attachment unavailable");
    await restored.env.RESTORE_FILES.put(
      file.object_key,
      await source.arrayBuffer(),
      {
        httpMetadata: source.httpMetadata,
        customMetadata: source.customMetadata,
      },
    );
    const copy = await restored.env.RESTORE_FILES.get(file.object_key);
    const digest = Array.from(
      new Uint8Array(
        await crypto.subtle.digest("SHA-256", await copy!.arrayBuffer()),
      ),
      (b) => b.toString(16).padStart(2, "0"),
    ).join("");
    if (digest !== file.checksum_sha256)
      throw new Error("Restored attachment checksum mismatch");
    verified++;
  }
  const report = {
    scope: "Local rehearsal only; original database and files remain in place",
    completedAt: new Date().toISOString(),
    counts,
    verifiedFiles: verified,
    foreignKeyViolations: 0,
  };
  await Bun.write(`${dir}/report.json`, JSON.stringify(report, null, 2));
  console.log(
    `Restore verified: ${tables.length} table counts, ${verified} file checksums. ${dir}/report.json`,
  );
} finally {
  await original.dispose();
  await restored.dispose();
}
