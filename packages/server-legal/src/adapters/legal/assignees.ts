import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { legalPeopleSource } from "@erpx/server-identity_access/public/legal-people-source";
import { legalPeopleActiveUnit, legalPeopleDepartmentInBranch } from "@erpx/server-organization/public/legal-people-scope";
import { ApiError } from "@erpx/server-runtime/public/http";
import { access } from "@erpx/server-legal/adapters/legal/access";
import { createLegalPeopleQuery } from "@erpx/server-legal/application/assignees";
export function legalPeople(db: SqlDatabase, a: WorkContext, raw: unknown = {}) { return createLegalPeopleQuery({ actor: a, require: () => requirePermission(a, "legal.read"), error: (status, code, message) => new ApiError(status, code, message), item: id => access(db, a, id), activeUnit: (id, kind) => legalPeopleActiveUnit(db, a.companyId, id, kind), departmentInBranch: (department, branch) => legalPeopleDepartmentInBranch(db, a.companyId, department, branch), people: (query, scope) => legalPeopleSource(db, a, query, scope) })(raw); }
