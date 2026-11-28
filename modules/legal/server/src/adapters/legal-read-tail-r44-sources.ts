import "server-only";
import contract from "./legal-read-tail-r44-sources.json" with { type: "json" };

/** Native-owned Legal read heads shared by the external-upload/share and
 * preservation/hold-target read families (round 44). Callers supply only
 * trusted projections, constant aliases and the fixed state inequality token;
 * every table, alias and join is owner-controlled in the JSON contract. */
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token))
      throw new Error("LEGAL_READ_TAIL_R44_CONTRACT_INVALID");
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
export const legalExternalUploadShareJoinHead = (
  projection: string,
  uploadAlias: string,
  shareAlias: string,
) =>
  fill(contract.externalUploadShare, {
    projection,
    uploadAlias,
    shareAlias,
  });
export const legalFilePreservationHoldTargetHead = (
  projection: string,
  targetAlias: string,
  holdAlias: string,
  inequality: string,
) =>
  fill(contract.filePreservationHoldTarget, {
    projection,
    targetAlias,
    holdAlias,
    inequality,
  });
