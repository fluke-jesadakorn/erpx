import "server-only";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import type { WorkContext } from "../../../identity_access/public/authorization";
import { requirePermission } from "../../../identity_access/public/iam-permissions";
import { legalPeopleSource } from "../../../identity_access/public/legal-people-source";
import { legalPeopleActiveUnit, legalPeopleDepartmentInBranch } from "../../../organization/public/legal-people-scope";
import { ApiError } from "../../../runtime/public/http";
import { access } from "./access";
import { createLegalPeopleQuery } from "../../application/assignees";
export function legalPeople(db: SqlDatabase, a: WorkContext, raw: unknown = {}) { return createLegalPeopleQuery({ actor: a, require: () => requirePermission(a, "legal.read"), error: (status, code, message) => new ApiError(status, code, message), item: id => access(db, a, id), activeUnit: (id, kind) => legalPeopleActiveUnit(db, a.companyId, id, kind), departmentInBranch: (department, branch) => legalPeopleDepartmentInBranch(db, a.companyId, department, branch), people: (query, scope) => legalPeopleSource(db, a, query, scope) })(raw); }
