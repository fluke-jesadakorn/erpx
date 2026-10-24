import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {employeeRequestDetail} from '@erpx/server-records/public/employee-requests';
import { purchaseRequestProgress } from "@erpx/server-procurement/public/purchase-request-progress";
import { requestApproverLegalName } from "@erpx/server-people_payroll/public/people-app-entrypoint-facts";
import { requestQuotation } from "@erpx/server-procurement/public/people-app-entrypoint-facts";
import { requestPageOptions } from "@erpx/server-records/public/request-page";
import {getEmployeeRequest} from '@erpx/server-records/public/employee-requests';

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function recordsPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    employeeRequestDetail: (...args: ReadArguments<typeof employeeRequestDetail>) => employeeRequestDetail(write, ...args),
    purchaseRequestProgress: (...args: ReadArguments<typeof purchaseRequestProgress>) => purchaseRequestProgress(write, ...args),
    requestApproverLegalName: (...args: ReadArguments<typeof requestApproverLegalName>) => requestApproverLegalName(write, ...args),
    requestQuotation: (...args: ReadArguments<typeof requestQuotation>) => requestQuotation(write, ...args),
    requestPageOptions: (...args: ReadArguments<typeof requestPageOptions>) => requestPageOptions(write, ...args),
    getEmployeeRequest: (...args: ReadArguments<typeof getEmployeeRequest>) => getEmployeeRequest(write, ...args),
  };
}
