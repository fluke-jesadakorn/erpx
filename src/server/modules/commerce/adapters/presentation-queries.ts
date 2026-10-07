import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { commerceEntrypointFinanceFacts } from "@/server/modules/finance/public/commerce-entrypoint-facts";
import { employeeRequestDetail } from "@/server/modules/records/public/employee-requests";
import { commerceEntrypointInventoryFacts } from "@/server/modules/inventory/public/commerce-entrypoint-facts";
import { commerceEntrypointRecordFacts } from "@/server/modules/records/public/commerce-entrypoint-facts";
import { getEmployeeRequest } from "@/server/modules/records/public/employee-requests";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function commercePresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    commerceEntrypointFinanceFacts: (...args: ReadArguments<typeof commerceEntrypointFinanceFacts>) => commerceEntrypointFinanceFacts(write, ...args),
    employeeRequestDetail: (...args: ReadArguments<typeof employeeRequestDetail>) => employeeRequestDetail(write, ...args),
    commerceEntrypointInventoryFacts: (...args: ReadArguments<typeof commerceEntrypointInventoryFacts>) => commerceEntrypointInventoryFacts(write, ...args),
    commerceEntrypointRecordFacts: (...args: ReadArguments<typeof commerceEntrypointRecordFacts>) => commerceEntrypointRecordFacts(write, ...args),
    getEmployeeRequest: (...args: ReadArguments<typeof getEmployeeRequest>) => getEmployeeRequest(write, ...args),
  };
}
