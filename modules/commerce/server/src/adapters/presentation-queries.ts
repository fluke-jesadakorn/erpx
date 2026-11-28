import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { commerceEntrypointFinanceFacts } from "@erpx/finance-server/public/commerce-entrypoint-facts";
import {employeeRequestDetail} from '@erpx/records-server/public/employee-requests-read';
import { commerceEntrypointInventoryFacts } from "@erpx/inventory-server/public/commerce-entrypoint-facts";
import { commerceEntrypointRecordFacts } from "@erpx/records-server/public/commerce-entrypoint-facts";
import {getEmployeeRequest} from '@erpx/records-server/public/employee-requests-read';

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
