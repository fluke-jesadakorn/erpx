/** G4 / retire.crm.read-tail-r36 byte-identical read heads.
 *
 * The opportunity-with-lead source anchor shared by the CRM attachment source
 * witness and the private attachment image owns one read token here. Callers
 * pass their own projection and tail so rendered SQL and bind order stay
 * byte-identical to the retired inline literals. Locks, command-receipt reads,
 * native command kinds and the r8-r35 families are untouched.
 */

/** Opportunity source anchor for the two CRM attachment reads. Callers pass the
 * projection (including its own trailing whitespace) and everything after the
 * fixed opportunity/lead identity join; the one read token lives here. */
export const crmOpportunitySourceAnchorSql = (projection: string, tail: string) =>
  `SELECT ${projection} FROM crm_opportunities opportunity JOIN crm_leads lead ON lead.id=opportunity.lead_id${tail}`;
