import "server-only";
/** Byte-identical disposed-document probe SQL shared by Legal owner adapters (round 20).
 * Text only: every call site still prepares and executes its own statement with unchanged binds. */
export const legalDisposedDocumentPrefix = "SELECT 1 FROM legal_disposals WHERE document_id";
