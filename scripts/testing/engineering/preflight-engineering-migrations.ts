import { getRuntime, closeRuntime } from "@erpx/server-runtime/adapters/runtime/env";
import { resourceSlots, registeredResources } from "@erpx/feature-runtime/lib/holding-registry";
import { inspectMigrations } from "@erpx/server-runtime/adapters/runtime/migrations";
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";

const env = getRuntime();
try {
  if (env.ERPX_ENVIRONMENT !== "local") throw new Error("DEVELOPMENT_ONLY");
  const reports = [];
  for (const slot of resourceSlots(env)) {
    const { write } = registeredResources(env,slot.id);
    if (!(write instanceof PostgresDatabase)) throw new Error("POSTGRES_DATABASE_REQUIRED");
    const database = await write.prepare("SELECT current_database() name").first<{name:string}>();
    if (!database || !/^erpx_dev_/.test(database.name)) throw new Error("DEVELOPMENT_DATABASE_REQUIRED");
    const duplicates = await write.prepare(`SELECT asset_id,count(*)::int work_count FROM maintenance_work_orders
      WHERE status NOT IN ('completed','cancelled') GROUP BY asset_id HAVING count(*)>1`)
      .all<{asset_id:string;work_count:number}>();
    const migrations = await inspectMigrations(write,"holding-write");
    reports.push({holdingId:slot.id,database:database.name,activeMaintenanceDuplicates:duplicates.results,migrations});
  }
  console.log(JSON.stringify({environment:"development",reports},null,2));
  if (reports.some(report=>report.activeMaintenanceDuplicates.length)) throw new Error("ACTIVE_MAINTENANCE_RECONCILIATION_REQUIRED");
} finally { await closeRuntime(); }
