import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { procurementInvoiceMatchForFinance } from "@erpx/server-procurement/public/finance-entrypoint-queries";
import { requestQuotationSource } from "@erpx/server-records/public/employee-requests";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function procurementPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    procurementInvoiceMatchForFinance: (...args: ReadArguments<typeof procurementInvoiceMatchForFinance>) => procurementInvoiceMatchForFinance(write, ...args),
    requestQuotationSource: (...args: ReadArguments<typeof requestQuotationSource>) => requestQuotationSource(write, ...args),
  };
}
