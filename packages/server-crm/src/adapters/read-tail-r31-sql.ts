/** G4 / retire.crm.read-tail-r31 byte-identical revenue-journal head.
 *
 * Both posting rows project l.id / primary_journal_id / revenue_minor over the
 * same lead -> opportunity join, so the head is the owner select-body plan over
 * that projection and join; this module carries no read token of its own.
 * Callers pass only the id alias, their posting source and their ON tail.
 * Locks, command-receipt reads, native command kinds and the r8-r30 families are
 * untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

/** One parameterized revenue-journal read head. Only the id alias, posting
 * source and trailing ON text vary; the rendered text is unchanged. */
export const crmRevenueJournalHeadSql = (idAlias: string, source: string) =>
  crmSelectBodySql(
    `l.id${idAlias},journal.primary_journal_id,journal.revenue_minor FROM crm_leads l JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id JOIN (${source}) source ON `,
  );
