import { getLocalProxy } from "../local/fixtures";
// Explicit demo configuration, separate from the additive production migration.
const platform = await getLocalProxy({
  persist: true,
  remoteBindings: false,
});
try {
  for (const [db, company, heads] of [
    [
      platform.env.H001_DB,
      "acme-co",
      [
        ["dept:bangkok:operations", "usr_nisa"],
        ["dept:new-york:operations", "usr_mali"],
      ],
    ],
    [
      platform.env.H002_DB,
      "northstar-co",
      [
        ["dept:toronto:operations", "usr_mia"],
        ["dept:rotterdam:operations", "usr_sam"],
      ],
    ],
  ] as const) {
    for (const [department, user] of heads) {
      const person = await db
        .prepare(
          "SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND status='active' AND merged_into_id IS NULL",
        )
        .bind(company, user)
        .first<{ id: string }>();
      if (!person) continue;
      await db
        .prepare(
          "INSERT INTO department_heads(department_id,company_id,person_id,updated_by,updated_at) VALUES(?,?,?,'local-demo-seed',?) ON CONFLICT DO NOTHING",
        )
        .bind(department, company, person.id, Date.now())
        .run();
    }
  }
  console.log(
    "Staff demo department heads configured; legacy levels remain Not set. Schedules remain paused until explicitly enabled.",
  );
} finally {
  await platform.dispose();
}
