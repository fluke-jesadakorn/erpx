import "server-only";
/** Byte-identical rate-limit assertion prefix shared by Legal owner adapters (round 19).
 * Text only: every call site still prepares and executes its own statement with unchanged binds. */
export const legalRateCountPrefix = "(SELECT count(*) FROM legal_";
