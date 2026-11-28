import 'server-only';

/** r57 additions: byte-identical read-tail SQL source. The authority-seat
 * EXISTS head is shared by the appointment-version assertion and the
 * relocation head guard. Each caller supplies its verbatim predicate tail;
 * bind arrays stay at the call sites. No r48/r50-r56 helper, lock or spaced
 * join constant is changed. */
export const authoritySeatExistsSql = (tail: string) =>
  `EXISTS(SELECT 1 FROM authority_seats ${tail})`;
