/** Add the missing October period for the Acme development rehearsal only. */
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { Pool } from "pg";
import { assertHospitalityDevelopmentTarget } from "../../src/lib/shared/hospitality-seed-target";

const manifestPath = process.env.ERPX_DATABASE_MANIFEST;
if (!manifestPath) throw new Error("ERPX_DATABASE_MANIFEST_REQUIRED");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const jobs = JSON.parse(await readFile(resolve(dirname(manifestPath), "databases-jobs.json"), "utf8"));
for (const target of [manifest, jobs]) assertHospitalityDevelopmentTarget(target, process.env.ERPX_ENVIRONMENT);
const guide = JSON.parse(await readFile(".scratch/dev/hospitality-acme/scenario-guide.json", "utf8"));
if (guide.companyId !== "acme-co" || guide.branchId !== "bangkok") throw new Error("ACME_BASELINE_REQUIRED");
const pool = new Pool({ connectionString: (process.argv.includes("--apply") ? jobs : manifest).databases.H001_DB, max: 1 });
const id = "period-acme-hospitality-ui-2026-10";
try {
  const connection = await pool.connect();
  try {
    await connection.query("BEGIN");
    if (process.argv.includes("--apply")) await connection.query("LOCK TABLE finance_periods IN SHARE ROW EXCLUSIVE MODE");
    const periods = (await connection.query("SELECT id,name,state,start_date,end_date FROM finance_periods WHERE company_id=$1 AND start_date<=$2 AND end_date>=$3", [guide.companyId, "2026-10-31", "2026-10-01"])).rows;
    const owned = periods.find(period => period.id === id);
    if (periods.length && (!owned || periods.length !== 1 || owned.state !== "open" || owned.start_date !== "2026-10-01" || owned.end_date !== "2026-10-31")) throw new Error("EXISTING_PERIOD_REQUIRES_FINANCE_REVIEW");
    const apply = process.argv.includes("--apply");
    if (apply && !owned) await connection.query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES($1,$2,$3,$4,$5,'open',$6)", [id, guide.companyId, "October 2026 · Acme demo rehearsal", "2026-10-01", "2026-10-31", Date.now()]);
    await connection.query("COMMIT");
    console.log(JSON.stringify({ developmentOnly: true, companyId: guide.companyId, id, startDate: "2026-10-01", endDate: "2026-10-31", result: owned ? "already present" : apply ? "created" : "preview: missing period; safe to add", policy: "No reopening, existing-period edits, payments or journals." }));
  } catch (error) {
    await connection.query("ROLLBACK");
    throw error;
  } finally { connection.release(); }
} finally { await pool.end(); }
