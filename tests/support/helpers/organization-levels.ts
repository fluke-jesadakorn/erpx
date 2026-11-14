import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/** Native-shape staff level seeding for integration fixtures. The production
 * levels command is served by the native Organization setup router. */
export async function installStaffLevels(
  database: SqlDatabase,
  actor: { companyId: string } | string,
  _commandKey?: string,
) {
  const companyId = typeof actor === "string" ? actor : actor.companyId;
  const titles = [
    "CEO",
    "Executive / Director",
    "Branch / Department Manager",
    "Supervisor",
    "Senior Staff",
    "Staff",
  ];
  await database.batch(
    titles.map((name, index) =>
      database
        .prepare(
          "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) VALUES(?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          `org-level-${companyId}-${index + 1}`,
          companyId,
          `ORG_L${index + 1}`,
          name,
          -index - 1,
          index + 1,
          Date.now(),
          Date.now(),
        ),
    ),
  );
}
