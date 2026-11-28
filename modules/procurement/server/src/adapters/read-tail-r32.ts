/** Procurement read-tail r32 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. r17-r31 helpers, FOR UPDATE/FOR SHARE and command_receipts stay untouched. */
/** Request-progress bill/payment aggregate. The company and joined order aliases
 * differ at each call site; the column list, FROM fragment, status predicate and
 * request-order link render verbatim. */
export const progressOrderAggregate = (
  columns: string,
  from: string,
  companyAlias: string,
  orderAlias: string,
  status: string,
  link: string,
) => `SELECT ${columns}
           FROM ${from}
           WHERE ${companyAlias}.company_id=? AND (${orderAlias}.id=? OR ${link}) AND ${status}`;
