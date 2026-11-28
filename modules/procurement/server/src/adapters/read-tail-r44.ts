/** Procurement read-tail r44 shared fragments. Text only: the builders render the
 * exact original predicates and witness frames so binds, ordering, locks and
 * writes are unchanged. r17-r43 helpers, FOR UPDATE/FOR SHARE locks and
 * command_receipts writes stay untouched. */
const receiptEvents = {
  'rfq-event-': {table:'procurement_rfq_events',fk:'rfq_id',actor:''},
  'bill-event-': {table:'procurement_bill_match_events',fk:'match_id',actor:' AND command.actor_id=event.actor_id'},
} as const;

/** `alias` is the command_receipts alias; `where` is the caller's verbatim
 * predicate text, including nested interpolations, rendered unchanged. */
export const commandReceiptMissing = (alias: string, where: string) =>
  `NOT EXISTS(SELECT 1 FROM command_receipts ${alias} WHERE ${where})`;

/** `prefix` selects the event/receipt witness family, `alias` is the outer
 * record alias, `predicate` is the caller's verbatim tail (the rfq arm carries
 * its actor guard there, the bill arm in the head) and `witness` is the r28
 * attachment-version frame rendered by the caller. */
export const commandReceiptEventWitness = (
  prefix: keyof typeof receiptEvents,
  alias: string,
  predicate: string,
  witness: string,
) => {
  const {table,fk,actor} = receiptEvents[prefix];
  return `(SELECT 1 FROM ${table} event JOIN command_receipts command ON event.id='${prefix}'||command.command_id
  WHERE event.${fk}=${alias}.id AND event.version<=${alias}.version AND command.authority_json::jsonb->>'companyId'=${alias}.company_id${actor}
${witness}
  AND ${predicate})`;
};
