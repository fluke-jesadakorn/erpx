import "server-only";
/** Byte-identical Legal read fragment families shared by owner adapters (round 39).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant aliases/references; no request text is interpolated. */
/** Disposed-document guard; the caller supplies the fixed alias and item reference. */
export const legalDisposedGuardSql = (alias: string, ref: string) =>
  `NOT EXISTS(SELECT 1 FROM legal_disposals ${alias} WHERE ${alias}.document_id=${ref})`;
/** Direct-hold witness on an owned target; the caller supplies the fixed holding alias. */
export const legalHoldOwnTargetSql = (alias: string) =>
  `EXISTS(SELECT 1 FROM legal_hold_targets own WHERE own.hold_id=${alias}.id AND own.item_id=?)`;
