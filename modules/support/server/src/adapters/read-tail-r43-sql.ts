/** Byte-identical parameterized column-projection head shared by the Support
 * incident workspace list reader and the ticket-portal valid-binding reader.
 * Only the projected columns and the trailing FROM/WHERE body vary; this
 * renderer owns the single shared read token and emits text only. Binds,
 * locks, native read kinds and execution stay at the call sites. */
export const readTailR43ColumnsFromSql = (columns: string, body: string) =>
  `SELECT ${columns} FROM ${body}`;
