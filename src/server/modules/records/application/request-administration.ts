import type { EmployeeRequest, employeeRequestDetail, listEmployeeRequests } from "../public/employee-requests";
import type { resourceRequestRoute } from "../../people_payroll/public/expense-routing";

export interface RequestAdministrationPorts {
  prepare(): Promise<void>;
  owner(): Promise<boolean>;
  list(): ReturnType<typeof listEmployeeRequests>;
  detail(id: string): ReturnType<typeof employeeRequestDetail>;
  route(request: EmployeeRequest): ReturnType<typeof resourceRequestRoute>;
}
export function createRequestAdministrationQuery(ports: RequestAdministrationPorts) {
  return async (record?: string) => {
    await ports.prepare();
    if (!await ports.owner()) return null;
    const rows = record ? [(await ports.detail(record)).request] : (await ports.list()).filter(row => !["draft", "returned", "closed", "cancelled", "rejected"].includes(row.state));
    const details = await Promise.all(rows.map(row => ports.detail(row.id)));
    const issues = record ? details : details.filter(detail => !!detail.blockedReason);
    return Promise.all(issues.map(async detail => ({ ...detail, route: detail.request.kind !== "service" ? await ports.route(detail.request) : null })));
  };
}
