import "server-only";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingBindingBySlug, holdingResources } from "../../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import { betaAccess } from "@erpx/server-identity_access/public/beta-access";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { currentAuthority, executiveEligible } from "@erpx/server-runtime/public/executive/access";
import type { TokenRow } from "@erpx/server-runtime/domain/executive/connections";
import { createExecutiveConnections, type ExecutiveConnectionUnit } from "@erpx/server-runtime/application/executive/connections";
import { executiveConnectionOrganizationFacts } from "@erpx/server-organization/public/executive-connection-facts";
import { executiveConnectionIdentityAccessFacts } from "@erpx/server-identity_access/public/executive-connection-facts";
const tokenColumns = "id,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at,revoked_at,last_used_at";
const sources = new WeakMap<ExecutiveConnectionUnit, SqlDatabase>();
/**
 * Read-only token resolution retained for the Executive MCP endpoint. Connection
 * create/list/revoke and the shared request budget are native Runtime routes;
 * this unit cannot write a token, audit or rate-limit row.
 */
export function executiveConnectionUnit(db: SqlDatabase): ExecutiveConnectionUnit {
    const unit: ExecutiveConnectionUnit = {
        organizationScopes: facts => executiveConnectionOrganizationFacts(db).organizationScopes(facts),
        tokenByHash: (facts: {
            value1: string;
        }) => db.prepare(`SELECT ${tokenColumns} FROM executive_external_tokens WHERE secret_hash=?`).bind(facts.value1).first<TokenRow>(),
        activeToken: (facts: {
            value1: string;
            value2: number;
        }) => db.prepare(`SELECT ${tokenColumns} FROM executive_external_tokens WHERE id=? AND revoked_at IS NULL AND expires_at>?`).bind(facts.value1, facts.value2).first<TokenRow>(),
        holdingBinding: (facts: {
            value1: string;
        }) => db.prepare("SELECT slug FROM holding_bindings WHERE holding_id=? AND status='active'").bind(facts.value1).first<{
            slug: string;
        }>(),
        activeGrant: facts => executiveConnectionIdentityAccessFacts(db).activeGrant(facts) };
    sources.set(unit, db);
    return unit;
}
function source(unit: ExecutiveConnectionUnit) { const db = sources.get(unit); if (!db)
    throw new Error("EXECUTIVE_DATABASE_REQUIRED"); return db; }
const service = createExecutiveConnections({ clock: { now: Date.now }, hash: sha256, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined, hasPermission, currentAuthority: (unit, actor) => currentAuthority(source(unit), actor), executiveEligible: (unit, actor) => executiveEligible(source(unit), actor), binding: holdingBindingBySlug, resource: async (id) => ({ write: executiveConnectionUnit((await holdingResources(id)).write) }), authority: (unit, ...args) => loadIamAuthority(source(unit), ...args), beta: (unit, companyId) => betaAccess(source(unit), companyId, new Headers()) });
export { type TokenRow, type ExecutiveConnectionGrant } from "@erpx/server-runtime/domain/executive/connections";
export function connectionEligible(db: SqlDatabase, actor: WorkContext) { return service.connectionEligible(executiveConnectionUnit(db), actor); }
export function connectionScopes(db: SqlDatabase, actor: WorkContext) { return service.connectionScopes(executiveConnectionUnit(db), actor); }
export function findExecutiveToken(control: SqlDatabase, authorization: string | null) { return service.findExecutiveToken(executiveConnectionUnit(control), authorization); }
export async function resolveExecutivePrincipal(control: SqlDatabase, token: TokenRow) { const principal = await service.resolveExecutivePrincipal(executiveConnectionUnit(control), token); return { ...principal, db: source(principal.db) }; }
export type ExecutivePrincipal = Awaited<ReturnType<typeof resolveExecutivePrincipal>>;
export function recheckExecutivePrincipal(control: SqlDatabase, principal: ExecutivePrincipal) { return service.recheckExecutivePrincipal(executiveConnectionUnit(control), { ...principal, db: executiveConnectionUnit(principal.db) }); }
