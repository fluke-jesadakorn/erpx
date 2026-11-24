/** G4 / retire.crm.read-tail-r31 byte-identical SELECT-head family.
 *
 * The sales-posting and hotel-posting revenue-journal rows share one read head:
 * both project l.id / journal.primary_journal_id / journal.revenue_minor over
 * the same crm_leads -> crm_opportunities join and the posting-source subquery.
 * The renderer below reproduces the pre-dedupe inline SQL byte-for-byte;
 * callers pass only the id alias, their posting source and their ON tail.
 * Locks, command-receipt reads, native command kinds and the r8-r30 families
 * are untouched.
 */

/** One parameterized revenue-journal read head. Only the id alias, posting
 * source and trailing ON text vary; the one read token here renders
 * byte-identically as both retired inline literals. */
export const crmRevenueJournalHeadSql = (idAlias: string, source: string) =>
  `SELECT l.id${idAlias},journal.primary_journal_id,journal.revenue_minor FROM crm_leads l JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id JOIN (${source}) source ON `;
