import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "../../identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

/** Person identity follows verified links, with the beta HR fallback explicitly opted in. */
export async function organizationPersonLinks(db: SqlDatabase, companyId: string, accounts: { id: string; email: string }[], allowBetaEmailFallback: boolean): Promise<{ accountId: string; personId: string | null }[]> {
  if (!accounts.length) return [];
  const links = (await db.prepare(`
      SELECT a.id AS account_id, person.id AS person_id
      FROM jsonb_to_recordset(?::jsonb) AS a(id text,email text)
      LEFT JOIN LATERAL (
        SELECT p.id FROM hr_people p
        WHERE p.company_id=? AND p.merged_into_id IS NULL
          AND (p.user_id=a.id OR EXISTS (
            SELECT 1 ${identityaccessFinalSources.organization_person_links_iam_members_m_from} WHERE m.company_id=p.company_id
              AND m.person_id=p.id AND m.user_id=a.id
          ) OR (?=1 AND p.user_id IS NULL AND lower(p.email)=lower(a.email)))
        ORDER BY CASE WHEN p.user_id IS NOT NULL THEN 0 ELSE 1 END,p.id LIMIT 1
      ) person ON true
    `).bind(JSON.stringify(accounts.map(({ id, email }) => ({ id, email }))), companyId, allowBetaEmailFallback ? 1 : 0).all<{ account_id: string; person_id: string | null }>()).results;
  return links.map(link => ({ accountId: link.account_id, personId: link.person_id }));
}
