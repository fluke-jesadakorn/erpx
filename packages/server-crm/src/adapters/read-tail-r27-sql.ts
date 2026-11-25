import {crmSingleKeySourceSql} from './read-tail-r37-sql';
import { crmProjectionFromSql } from './read-tail-r46-sql';

/** G4 / retire.crm.read-tail-r27 byte-identical read families.
 *
 * The renderers below reproduce the pre-dedupe inline SQL exactly. The
 * opportunity owner read and its attachment version guard share one anchor
 * renderer; the two campaign option projections share another. Callers keep
 * their own eligibility expression, campaign source and binds verbatim.
 * All transaction lock and row-lock statements stay inline in the calling
 * adapters. Native command kinds and r8-r26 families are untouched.
 */

/** Shared opportunity-owner anchor. Only the projection, predicate tail and
 * EXISTS wrapper vary; the one read token lives here and renders byte-for-byte
 * as both retired inline literals. */
const crmOpportunityOwnerAnchor = (projection: string, predicates: string, exists = false) => {
  const read = crmProjectionFromSql(projection, `crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id${predicates}`);
  return exists ? `EXISTS(${read})` : read;
};

/** attachments-policy-facts.ts readCrmOpportunity projection. */
export const crmOpportunityOwnerReadSql = (eligible: string) => crmOpportunityOwnerAnchor(
  `l.company_id,l.branch_id,l.department_id,l.owner_id,o.version,
      CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible
   `,
  `
    WHERE o.id=? AND l.company_id=?
      AND (?::text IS NULL OR l.branch_id=?)
      AND (?::text IS NULL OR l.department_id=?)`,
);

/** attachments-service-facts.ts opportunity attachment owner version guard. */
export const crmOpportunityOwnerGuardSql = (eligible: string) => crmOpportunityOwnerAnchor(
  '1',
  `
      WHERE o.id=? AND l.company_id=? AND o.version=? AND l.owner_id=?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
        AND ${eligible}`,
  true,
);

/** Campaign option projection. Leads selects the name column; analytics aliases
 * code to name. Both render byte-identically from the same projection shape. */
export const crmCampaignOptionsSql = (source: string, alias: string, aliasCode = false) =>
  crmSingleKeySourceSql(`id,code${aliasCode ? ' name' : ',name'}`,source,alias,' ORDER BY code');
