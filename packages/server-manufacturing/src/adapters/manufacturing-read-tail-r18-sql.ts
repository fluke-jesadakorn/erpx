/** Byte-identical Manufacturing read-tail r18 SQL text shared across the
 * purchased production-record UNION branches. Text only: every branch keeps its
 * own literal kind/record_type/module/capability projections, witness column,
 * source rows and alias; nothing is cached or executed here and it stays
 * outside the RD_* read registry. The leading branch carries its `::text`
 * casts inside the caller arguments so the emitted SQL stays byte for byte. */
export const manufacturingPurchaseRecordBranchSelect = (
  kind: string,
  recordType: string,
  module: string,
  capability: string,
  witness: string,
  source: string,
  alias: string,
) =>
  `SELECT ${kind},${recordType},${module},${capability},id,company_id,branch_id,advanced,${witness} FROM (${source}) ${alias}`;
