import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { serviceOwner } from "@erpx/support-server/public/service-access";
import { resourceRequestRoute } from "@erpx/people-payroll-server/public/expense-routing";
import {listEmployeeRequests} from '@erpx/records-server/public/employee-requests';
import {employeeRequestDetail} from '@erpx/records-server/public/employee-requests-read';
import { createRequestAdministrationQuery } from "@erpx/records-server/application/request-administration";

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
