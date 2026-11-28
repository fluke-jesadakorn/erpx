/** Byte-identical Finance read-tail SQL shared across captured journal-line
 * evidence projections. Text only: every call site still prepares the rendered
 * text and binds its own values. Lock-taking (FOR UPDATE / FOR SHARE) variants
 * stay inline. */
export const journalLineEvidenceSql = (journalRef: string) =>
  `(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM finance_journal_lines line WHERE line.journal_id=${journalRef})`;
