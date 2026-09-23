/** Run only the PMS worker in one verified disposable browser demo database. */
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { PostgresDatabase } from "../../src/server/runtime/postgres";
import { drainPms } from "../../src/server/pms";

const run = process.argv[2];
if (!run || !/^[a-f0-9]{12}$/.test(run)) throw new Error("DEMO_RUN_REQUIRED");
const directory = resolve(`.runtime/browser/${run}`);
const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as {
  run: string;
  prefix: string;
  fixture: { hospitality: { propertyId: string } };
};
const manifest = JSON.parse(await readFile(resolve(directory, "databases.json"), "utf8")) as {
  databases: { H002_DB: string };
};
const databaseUrl = new URL(manifest.databases.H002_DB);
if (
  state.run !== run ||
  state.prefix !== `erpx_browser_${run}` ||
  databaseUrl.hostname !== "127.0.0.1" ||
  databaseUrl.port !== "5432" ||
  databaseUrl.pathname !== `/erpx_browser_${run}_h002_db`
) throw new Error("ISOLATED_DEMO_DATABASE_REQUIRED");

const db = new PostgresDatabase({ connectionString: databaseUrl.href, max: 1 });
try {
  for (let step = 0; step < 8; step++) {
    const job = await db.prepare("SELECT state FROM pms_jobs WHERE property_id=? ORDER BY updated_at DESC LIMIT 1")
      .bind(state.fixture.hospitality.propertyId).first<{ state: string }>();
    if (!job || ["completed", "blocked"].includes(job.state)) break;
    await drainPms(db);
  }
  const job = await db.prepare("SELECT state,error_code FROM pms_jobs WHERE property_id=? ORDER BY updated_at DESC LIMIT 1")
    .bind(state.fixture.hospitality.propertyId).first<{ state: string; error_code: string | null }>();
  const property = await db.prepare("SELECT state,business_date FROM pms_properties WHERE id=?")
    .bind(state.fixture.hospitality.propertyId).first<{ state: string; business_date: string }>();
  process.stdout.write(`${JSON.stringify({ job, property })}\n`);
} finally {
  await db.close();
}
