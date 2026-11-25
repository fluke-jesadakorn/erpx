/** G4 / retire.crm.read-tail-r26 byte-identical read families.
 *
 * Both source reads are the owner select-body plan over the caller's column
 * list and join/where tail, so this module carries no read token of its own.
 * Each call site keeps its own columns, joins, binds and access guards verbatim.
 * All transaction lock and row-lock statements stay inline in the calling
 * adapters. r8-r25 families are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

/** Guest-link reservation/stay source. Callers pass their own column list, the
 * two Hotel projection sources, an optional stay join, the order/limit tail and
 * the line separator used by the original literal ('' inline, '\n  ' wrapped). */
export const crmGuestLinkReservationSourceSql = (
  columns: string,
  properties: string,
  records: string,
  stayJoin: string,
  tail: string,
  nl: string,
) => crmSelectBodySql(
  `${columns}${nl}FROM crm_guest_links g JOIN (${properties}) p ON p.id=g.property_id AND p.company_id=g.company_id${nl}JOIN (${records}) r ON r.property_id=p.id AND r.kind='reservation' AND r.data_json::jsonb->>'guestId'=g.guest_id${stayJoin}${nl}WHERE g.sharing_state='granted' AND g.contact_id=? AND g.company_id=? AND (?::text IS NULL OR p.branch_id=?)${tail}`,
);

/** Case-event lead-read join. Callers pass their own column list, predicate
 * tail, lead-projection scope and order tail; the rendered text is unchanged. */
export const crmCaseEventLeadReadSql = (
  columns: string,
  predicates: string,
  crmWhere: string,
  tail = '',
) => crmSelectBodySql(
  `${columns} FROM crm_case_events event JOIN crm_leads l ON l.id=event.lead_id WHERE l.company_id=? AND ${predicates} AND ${crmWhere}${tail}`,
);
