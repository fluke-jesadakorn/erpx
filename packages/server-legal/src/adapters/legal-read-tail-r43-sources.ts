import "server-only";
/** Byte-identical parameterized envelope/signer read head shared by the Legal
 * envelope detail, portal envelope list, portal actionable-envelope probe,
 * operations envelope read and the sent-envelope existence probe. Callers
 * supply only the fixed projection and the trusted remaining clause; no table,
 * alias or SQL is accepted from a request. */
export const legalEnvelopeSignerReadHead = (projection: string) =>
  `SELECT ${projection} FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE `;
