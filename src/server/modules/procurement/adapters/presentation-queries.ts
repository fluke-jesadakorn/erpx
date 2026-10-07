import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { procurementInvoiceMatchForFinance } from "@/server/modules/procurement/public/finance-entrypoint-queries";
import { requestQuotationSource } from "@/server/modules/records/public/employee-requests";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function procurementPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    procurementInvoiceMatchForFinance: (...args: ReadArguments<typeof procurementInvoiceMatchForFinance>) => procurementInvoiceMatchForFinance(write, ...args),
    requestQuotationSource: (...args: ReadArguments<typeof requestQuotationSource>) => requestQuotationSource(write, ...args),
  };
}
