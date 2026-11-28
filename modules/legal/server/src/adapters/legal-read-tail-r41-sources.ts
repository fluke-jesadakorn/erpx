import "server-only";
/** Byte-identical parameterized read head shared by every single-line Legal
 * portal share/document read. Callers supply only the fixed projection and the
 * remaining clause; no table, alias or SQL is accepted from an untrusted input. */
export const legalShareDocumentReadHead = (projection: string) =>
  `SELECT ${projection} FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id `;
