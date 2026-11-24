import 'server-only';

/** Canonical command-receipt version guard shared by the Operations task and
 *  schedule pending projections. The record/operation prefix, company, optional
 *  result-id clause, result state and version are the only varying bytes; each
 *  rendering is byte-identical to the retired inline literal. Binds unchanged. */
export const operationsReceiptVersionExists = (
  prefix: string,
  company: string,
  idClause: string,
  state: string,
  version: string,
) =>
  `EXISTS(SELECT 1 FROM command_receipts receipt WHERE ${prefix} AND receipt.authority_json::jsonb->>'companyId'=${company}${idClause} AND receipt.result_json::jsonb->>'state'=${state} AND (receipt.result_json::jsonb->>'version')::bigint<=${version})`;
