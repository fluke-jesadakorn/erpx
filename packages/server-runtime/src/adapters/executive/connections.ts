import "server-only";
import { randomBytes, randomUUID } from "node:crypto";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingBindingBySlug, holdingResources } from "../../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
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
export function executiveConnectionUnit(db: SqlDatabase): ExecutiveConnectionUnit {
    const unit: ExecutiveConnectionUnit = { transactional: isTransactionalSqlDatabase(db), transaction: action => { if (!isTransactionalSqlDatabase(db))
            throw new Error("EXECUTIVE_TRANSACTION_REQUIRED"); return db.transaction(tx => action(executiveConnectionUnit(tx))); },
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
        activeGrant: facts => executiveConnectionIdentityAccessFacts(db).activeGrant(facts),
        listTokens: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare(`SELECT ${tokenColumns} FROM executive_external_tokens WHERE user_id=? AND holding_id=? AND company_id=? ORDER BY created_at DESC LIMIT 100`).bind(facts.value1, facts.value2, facts.value3).all<TokenRow>(),
        selectedGrant: facts => executiveConnectionIdentityAccessFacts(db).selectedGrant(facts),
        lockPrincipal: (facts: {
            value1: string;
        }) => db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,7092))").bind(facts.value1).run(),
        countActive: (facts: {
            value1: string;
            value2: string;
            value3: number;
        }) => db.prepare("SELECT count(*)::integer count FROM executive_external_tokens WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND expires_at>?").bind(facts.value1, facts.value2, facts.value3).first<{
            count: number;
        }>(),
        insertToken: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: string;
            value6: string;
            value7: string;
            value8: number;
            value9: string;
            value10: "holding" | "branch" | "department";
            value11: string | null;
            value12: string | null;
            value13: string | null;
            value14: string;
            value15: number;
            value16: number;
        }) => db.prepare(`INSERT INTO executive_external_tokens(id,secret_hash,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING RETURNING id`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value9, facts.value10, facts.value11, facts.value12, facts.value13, facts.value14, facts.value15, facts.value16).first(),
        auditCreate: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: string;
            value6: string | null;
            value7: string | null;
            value8: number;
        }) => db.prepare(`INSERT INTO executive_mcp_audit(id,token_id,user_id,company_id,assignment_id,branch_id,department_id,action,outcome,code,started_at,duration_ms)
      VALUES(?,?,?,?,?,?,?,'connection.create','success','OK',?,0)`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8).run(),
        revokeFailed: (facts: {
            value1: number;
            value2: string;
        }) => db.prepare("UPDATE executive_external_tokens SET revoked_at=? WHERE id=?").bind(facts.value1, facts.value2).run(),
        revokeOwned: (facts: {
            value1: number;
            value2: string;
            value3: string;
            value4: string;
            value5: string;
        }) => db.prepare("UPDATE executive_external_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE id=? AND user_id=? AND holding_id=? AND company_id=? RETURNING id").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5).first<{
            id: string;
        }>(),
        consumeRate: (facts: {
            value1: string;
            value2: string;
            value3: number;
        }) => db.prepare(`INSERT INTO executive_mcp_rate_limits(user_id,company_id,window_start,requests) VALUES(?,?,?,1)
    ON CONFLICT(user_id,company_id) DO UPDATE SET window_start=excluded.window_start,
      requests=CASE WHEN executive_mcp_rate_limits.window_start=excluded.window_start THEN executive_mcp_rate_limits.requests+1 ELSE 1 END
    RETURNING requests`).bind(facts.value1, facts.value2, facts.value3).first<{
            requests: number;
        }>() };
    sources.set(unit, db);
    return unit;
}
function source(unit: ExecutiveConnectionUnit) { const db = sources.get(unit); if (!db)
    throw new Error("EXECUTIVE_DATABASE_REQUIRED"); return db; }
const service = createExecutiveConnections({ clock: { now: Date.now }, identifiers: { next: randomUUID }, secret: () => randomBytes(32).toString("base64url"), hash: sha256, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined, hasPermission, currentAuthority: (unit, actor) => currentAuthority(source(unit), actor), executiveEligible: (unit, actor) => executiveEligible(source(unit), actor), binding: holdingBindingBySlug, resource: async (id) => ({ write: executiveConnectionUnit((await holdingResources(id)).write) }), authority: (unit, ...args) => loadIamAuthority(source(unit), ...args), beta: (unit, companyId) => betaAccess(source(unit), companyId, new Headers()) });
export { type TokenRow, type ExecutiveConnectionGrant } from "@erpx/server-runtime/domain/executive/connections";
export function connectionEligible(db: SqlDatabase, actor: WorkContext) { return service.connectionEligible(executiveConnectionUnit(db), actor); }
export function connectionScopes(db: SqlDatabase, actor: WorkContext) { return service.connectionScopes(executiveConnectionUnit(db), actor); }
export function findExecutiveToken(control: SqlDatabase, authorization: string | null) { return service.findExecutiveToken(executiveConnectionUnit(control), authorization); }
export async function resolveExecutivePrincipal(control: SqlDatabase, token: TokenRow) { const principal = await service.resolveExecutivePrincipal(executiveConnectionUnit(control), token); return { ...principal, db: source(principal.db) }; }
export type ExecutivePrincipal = Awaited<ReturnType<typeof resolveExecutivePrincipal>>;
export function recheckExecutivePrincipal(control: SqlDatabase, principal: ExecutivePrincipal) { return service.recheckExecutivePrincipal(executiveConnectionUnit(control), { ...principal, db: executiveConnectionUnit(principal.db) }); }
export function listExternalConnections(control: SqlDatabase, db: SqlDatabase, actor: WorkContext) { return service.listExternalConnections(executiveConnectionUnit(control), executiveConnectionUnit(db), actor); }
export function createExternalConnection(control: SqlDatabase, db: SqlDatabase, actor: WorkContext, raw: unknown, commandKey: string) { return service.createExternalConnection(executiveConnectionUnit(control), executiveConnectionUnit(db), actor, raw, commandKey); }
export function revokeExternalConnection(control: SqlDatabase, db: SqlDatabase, actor: WorkContext, id: string) { return service.revokeExternalConnection(executiveConnectionUnit(control), executiveConnectionUnit(db), actor, id); }
export function consumeExecutiveRateLimit(control: SqlDatabase, token: TokenRow) { return service.consumeExecutiveRateLimit(executiveConnectionUnit(control), token); }
