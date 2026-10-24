import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { serviceOwner } from "@erpx/server-support/public/service-access";
import { resourceRequestRoute } from "@erpx/server-people_payroll/public/expense-routing";
import {listEmployeeRequests,employeeRequestDetail} from '@erpx/server-records/public/employee-requests';
import { createRequestAdministrationQuery } from "@erpx/server-records/application/request-administration";

export function requestAdministrationData(actor: WorkContext, record?: string) {
  let db: SqlDatabase;
  return createRequestAdministrationQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    owner: () => serviceOwner(db, actor),
    list: () => listEmployeeRequests(db, actor),
    detail: id => employeeRequestDetail(db, actor, id),
    route: request => resourceRequestRoute(db, actor.companyId, request.requester_user_id, request.created_by),
  })(record);
}
