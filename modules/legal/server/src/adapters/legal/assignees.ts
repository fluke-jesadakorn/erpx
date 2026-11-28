import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { legalPeopleSource } from "@erpx/identity-access-server/public/legal-people-source";
import { legalPeopleActiveUnit, legalPeopleDepartmentInBranch } from "@erpx/organization-server/public/legal-people-scope";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { access } from "@erpx/legal-server/adapters/legal/access";
import { createLegalPeopleQuery } from "@erpx/legal-server/application/assignees";
export function legalPeople(db: SqlDatabase, a: WorkContext, raw: unknown = {}) { return createLegalPeopleQuery({ actor: a, require: () => requirePermission(a, "legal.read"), error: (status, code, message) => new ApiError(status, code, message), item: id => access(db, a, id), activeUnit: (id, kind) => legalPeopleActiveUnit(db, a.companyId, id, kind), departmentInBranch: (department, branch) => legalPeopleDepartmentInBranch(db, a.companyId, department, branch), people: (query, scope) => legalPeopleSource(db, a, query, scope) })(raw); }
