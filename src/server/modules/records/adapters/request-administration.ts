import "server-only";
import { holdingResources } from "../../runtime/public/runtime";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { serviceOwner } from "../../support/public/service-access";
import { resourceRequestRoute } from "../../people_payroll/public/expense-routing";
import { listEmployeeRequests, employeeRequestDetail } from "../public/employee-requests";
import { createRequestAdministrationQuery } from "../application/request-administration";

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
