import { runtimeEnv, holdingResources } from "./runtime";
import { canManagePeople, directoryPerson } from "./people-directory";
import type { WorkContext } from "./authorization";
export async function companyMembers(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId),
    env = await runtimeEnv(),
    manager = canManagePeople(actor);
  const members = (
    await write
      .prepare(
        `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IS NOT NULL AND ((revoked_at IS NULL AND valid_from<=? AND valid_until>?) OR ?=1) AND (?::text IS NULL OR scope_kind='holding' OR COALESCE(branch_id,(SELECT branch_id FROM organization_nodes WHERE id=scope_node_id))=?) ORDER BY user_id`,
      )
      .bind(
        actor.companyId,
        Date.now(),
        Date.now(),
        manager ? 1 : 0,
        actor.branchId,
        actor.branchId,
      )
      .all<{ user_id: string }>()
  ).results;
  const rows: {
    id: string;
    name: string;
    email: string;
    personId: string | null;
  }[] = [];
  for (let offset = 0; offset < members.length; offset += 80) {
    const ids = members.slice(offset, offset + 80).map((m) => m.user_id);
    const accounts = (
      await env.CONTROL_DB.prepare(
        `SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`,
      )
        .bind(...ids)
        .all<{ id: string; name: string; email: string }>()
    ).results;
    // Resolve the whole control-database batch in one holding query. Keep the
    // verified-link and beta email fallback rules identical to profile lookup.
    const links = accounts.length ? (await write.prepare(`
      SELECT a.id AS account_id, person.id AS person_id
      FROM jsonb_to_recordset(?::jsonb) AS a(id text,email text)
      LEFT JOIN LATERAL (
        SELECT p.id FROM hr_people p
        WHERE p.company_id=? AND p.merged_into_id IS NULL
          AND (p.user_id=a.id OR EXISTS (
            SELECT 1 FROM iam_members m WHERE m.company_id=p.company_id
              AND m.person_id=p.id AND m.user_id=a.id
          ) OR (?=1 AND p.user_id IS NULL AND lower(p.email)=lower(a.email)))
        ORDER BY CASE WHEN p.user_id IS NOT NULL THEN 0 ELSE 1 END,p.id LIMIT 1
      ) person ON true
    `).bind(JSON.stringify(accounts.map(({id,email})=>({id,email}))),
      actor.companyId, actor.beta && manager ? 1 : 0)
      .all<{account_id:string;person_id:string|null}>()).results : [];
    const personIds = new Map(links.map(link=>[link.account_id,link.person_id]));
    for (const account of accounts) {
      rows.push({
        ...account,
        email: manager || account.id === actor.userId ? account.email : "",
        personId: personIds.get(account.id) ?? null,
      });
    }
  }
  return rows.sort((a, b) => a.name.localeCompare(b.name));
}
export async function memberProfile(
  actor: WorkContext,
  personId: string | null,
) {
  return personId ? directoryPerson(actor, personId) : null;
}
