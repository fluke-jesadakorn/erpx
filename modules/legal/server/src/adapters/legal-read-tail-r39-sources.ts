import "server-only";
import contract from "./legal-read-tail-r39-sources.json" with { type: "json" };

/** Native-owned Legal r39 fragment families. Callers pass trusted constant
 * aliases/references; no request text is interpolated. Every table, join and
 * bind is owner-controlled in the JSON contract. */
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token))
      throw new Error("LEGAL_READ_TAIL_R39_CONTRACT_INVALID");
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
/** Disposed-document guard; the caller supplies the fixed alias and item reference. */
export const legalDisposedGuardSql = (alias: string, ref: string) =>
  fill(contract.disposedGuard, { alias, ref });
/** Direct-hold witness on an owned target; the caller supplies the fixed holding alias. */
export const legalHoldOwnTargetSql = (alias: string) =>
  fill(contract.holdOwnTarget, { alias });
