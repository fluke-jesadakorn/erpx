import {recordDirectoryVerification} from "../../workflow/public/identity-command-effects";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import * as people from '@/server/modules/people_payroll/public/directory-operation-facts';
import * as organization from '@/server/modules/organization/public/directory-operation-facts';
import { canonical, connectorMappingInput } from '../domain/directory-operations';
import { createDirectoryOperations, type OperationTransaction } from '../application/directory-operations';
const clients = new WeakMap<OperationTransaction, PoolClient>();
import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import { assertCompanyScope, type IamActor } from "@/server/modules/identity_access/public/iam-context";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { iamCommand } from "@/server/modules/identity_access/public/iam-transactions";
import { requireFreshMfa } from "@/server/modules/identity_access/public/iam-mfa";
import { proposeIamReview } from "@/server/modules/identity_access/public/iam-service";
import { readAutomationInventory, validateObjectTarget, applyDirectoryAction } from "@/server/modules/identity_access/public/directory-provider";
import { type DirectoryChangeInput } from "@/lib/identity_access/directory-operations";
import { ApiError } from "@/server/modules/runtime/public/http";
const hash = (v: unknown) => createHash("sha256").update(JSON.stringify(canonical(v))).digest("hex");
export async function directoryAutomationData(db: PostgresDatabase, actor: IamActor) { return operationService(db).directoryAutomationData(actor); }
export async function refreshDirectoryInventory(db: PostgresDatabase, actor: IamActor, id: string, version: number, key: string) { return operationService(db).refreshDirectoryInventory(actor, id, version, key); }
export { connectorMappingInput, affectedObjects } from '../domain/directory-operations';
export async function configureDirectoryAutomation(db: PostgresDatabase, actor: IamActor, input: {
    connectionId: string;
    expectedVersion: number;
    writeEnabled: boolean;
    mappings?: z.infer<typeof connectorMappingInput>[];
}, key: string) { return operationService(db).configureDirectoryAutomation(actor, input, key); }
export async function prepareDirectoryPreview(c: PoolClient, actor: IamActor, input: DirectoryChangeInput) { return operationService().prepareDirectoryPreview(operationTx(c), actor, input); }
export async function previewDirectoryChange(db: PostgresDatabase, actor: IamActor, input: DirectoryChangeInput) { return operationService(db).previewDirectoryChange(actor, input); }
export async function submitDirectoryChange(db: PostgresDatabase, actor: IamActor, raw: DirectoryChangeInput, token: string, key: string, direct = false) { return operationService(db).submitDirectoryChange(actor, raw, token, key, direct); }
export async function applyDirectoryOperationReview(c: PoolClient, actor: IamActor, p: Record<string, unknown>, decision: "approve" | "reject" = "approve") { return operationService().applyDirectoryOperationReview(operationTx(c), actor, p, decision); }
/** Transport/MFA retry keeps its journal and completed steps. Stale previews cannot be retried. */
export async function retryDirectoryOperation(db: PostgresDatabase, actor: IamActor, id: string, version: number, key: string) { return operationService(db).retryDirectoryOperation(actor, id, version, key); }
export async function cancelDirectoryOperation(db: PostgresDatabase, actor: IamActor, id: string, version: number, key: string) { return operationService(db).cancelDirectoryOperation(actor, id, version, key); }
/** A session advisory lock serializes external operations per connection across workers. */
export async function drainDirectoryOperations(db: PostgresDatabase, limit = 10) { return operationService(db).drainDirectoryOperations(limit); }
function operationTx(c: PoolClient): OperationTransaction {
    const tx: OperationTransaction = { connection: async (facts) => {
            const { id, actor } = facts;
            return (await c.query("SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2", [id, actor.companyId])).rows[0] ?? null;
        },
        recordInventory: async (facts) => {
            const { id, actor, version, inventory, observedNow } = facts;
            return void await c.query("INSERT INTO people_directory_state(connection_id,company_id,tested_version,inventory,observed_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(connection_id) DO UPDATE SET tested_version=excluded.tested_version,inventory=excluded.inventory,observed_at=excluded.observed_at,write_enabled=CASE WHEN people_directory_state.tested_version=excluded.tested_version AND people_directory_state.inventory->>'providerVersion'=excluded.inventory->>'providerVersion' THEN people_directory_state.write_enabled ELSE false END", [id, actor.companyId, version, JSON.stringify(inventory), observedNow]);
        },
        lockState: async (facts) => {
            const { row } = facts;
            return (await c.query("SELECT * FROM people_directory_state WHERE connection_id=$1 FOR UPDATE", [row.id])).rows[0] ?? null;
        },
        ownedAttribute: async (facts) => {
            const { actor, m } = facts;
            return (await c.query("SELECT definition FROM um_attributes WHERE company_id=$1 AND id=$2", [actor.companyId, m.attributeId])).rows[0] ?? null;
        },
        configureState: async (facts) => {
            const { input, mappings, row } = facts;
            return void await c.query("UPDATE people_directory_state SET write_enabled=$1,definition=$2,version=version+1 WHERE connection_id=$3", [input.writeEnabled, JSON.stringify({ mappings }), row.id]);
        },
        state: async (facts) => {
            const { row } = facts;
            return (await c.query("SELECT * FROM people_directory_state WHERE connection_id=$1", [row.id])).rows[0] ?? null;
        },
        identityLinks: async (facts) => {
            const { row, ids } = facts;
            return (await c.query("SELECT i.*,c.company_id,c.config FROM iam_identities i JOIN iam_connections c ON c.id=i.connection_id WHERE c.config->>'authority'=$1 AND i.external_id=ANY($2::text[]) ORDER BY c.company_id,i.id", [row.config.authority, ids])).rows;
        },
        lifecycle: (facts) => people.lifecycle(c, facts),
        verifiedLifecycleAccount: async (facts) => {
            const { row, action, actor, lifecycle } = facts;
            return (await c.query("SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE i.connection_id=$1 AND i.external_id=$2 AND m.company_id=$3 AND m.person_id=$4 AND m.status='active'", [row.id, action.objectId, actor.companyId, lifecycle.person_id])).rowCount;
        },
        relatedConnections: async (facts) => {
            const { row } = facts;
            return (await c.query("SELECT id,company_id FROM iam_connections WHERE config->>'authority'=$1", [row.config.authority])).rows;
        },
        affectedPositions: (facts) => organization.affectedPositions(c, facts),
        companyNames: (facts) => organization.companyNames(c, facts),
        protectedAccount: async (facts) => {
            const { i, observedNow, actor } = facts;
            return (await c.query("SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 AND a.role='holding_owner' AND iam_assignment_active(a.id,$3)) OR EXISTS(SELECT 1 FROM iam_members m WHERE m.user_id=$1 AND m.company_id=$2 AND m.login_policy='emergency' AND m.status='active') protected,iam_same_person($1,$4,$2) self", [i.user_id, i.company_id, observedNow, actor.userId])).rows[0] ?? null;
        },
        affectedPeople: (facts) => people.affectedPeople(c, facts),
        previewMembershipClaims: async (facts) => {
            const { row, action } = facts;
            return (await c.query(`SELECT s.member_id,s.source_id FROM people_directory_membership_sources s LEFT ${peoplepayrollSources.directory_operations_people_applied_packages_ap_join} ON ap.id=s.application_id WHERE s.connection_id=$1 AND s.group_id=$2 AND s.member_id=ANY($3::text[]) AND s.application_id IS NOT NULL AND ap.ended_at IS NULL`, [row.id, action.objectId, action.memberIds])).rows;
        },
        directCompanyAuthority: async (facts) => {
            const { companyId, actor, observedNow } = facts;
            return (await c.query("SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.directory.apply')", [companyId, actor.userId, observedNow])).rowCount;
        },
        insertOperation: async (facts) => {
            const { id, actor, p, input, review, observedNow, direct } = facts;
            return void await c.query("INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,state_version,status,definition,evidence,review_id,created_by,assignment_id,created_at,updated_at,approval_mode,mfa_session_id) VALUES($1,$2,$3,$4,$5,$12,$6,$7,$8,$9,$10,$11,$11,$13,$14)", [
                id,
                actor.companyId,
                p.row.id,
                p.row.version,
                p.state.version,
                JSON.stringify(input),
                JSON.stringify(p.evidence),
                review?.id ?? null,
                actor.userId,
                actor.assignmentId,
                observedNow,
                direct ? "queued" : "pending_review",
                direct ? "direct" : "reviewed",
                direct ? actor.sessionId : null,
            ]);
        },
        lockPendingOperation: async (facts) => {
            const { p, actor } = facts;
            return (await c.query("SELECT * FROM people_directory_operations WHERE id=$1 AND company_id=$2 AND status='pending_review' FOR UPDATE", [p.operationId, actor.companyId])).rows[0] ?? null;
        },
        beneficiaryIsReviewer: async (facts) => {
            const { userId, actor, companyId } = facts;
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
                userId,
                actor.userId,
                companyId,
            ])).rows[0] ?? null;
        },
        reviewCompanyAuthority: async (facts) => {
            const { companyId, actor, observedNow } = facts;
            return (await c.query("SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.directory.manage')", [companyId, actor.userId, observedNow])).rowCount;
        },
        decideOperation: async (facts) => {
            const { decision, observedNow, job } = facts;
            return void await c.query("UPDATE people_directory_operations SET status=$1,version=version+1,updated_at=$2 WHERE id=$3", [decision === "approve" ? "queued" : "rejected", observedNow, job.id]);
        },
        resumeOperation: async (facts) => {
            const { id, actor, version } = facts;
            return (await c.query("UPDATE people_directory_operations SET status='queued',mfa_session_id=$5,next_attempt_at=0,version=version+1 WHERE id=$1 AND company_id=$2 AND version=$3 AND created_by=$4 AND approval_mode='direct' AND (status='retry' OR status='blocked' AND last_error='FRESH_MFA_REQUIRED') RETURNING provision_id", [id, actor.companyId, version, actor.userId, actor.sessionId])).rows[0] ?? null;
        },
        resumeProvisioning: (facts) => people.resumeProvisioning(c, facts),
        cancelOperation: async (facts) => {
            const { observedNow, id, actor, version } = facts;
            return (await c.query("UPDATE people_directory_operations SET status='cancelled',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4 AND attempts=0 AND status IN ('pending_review','queued','blocked') RETURNING review_id", [observedNow, id, actor.companyId, version])).rows[0] ?? null;
        },
        cancelReview: async (facts) => {
            const { job } = facts;
            return void await c.query("UPDATE iam_reviews SET status='failed',version=version+1 WHERE id=$1 AND status='pending'", [job.review_id]);
        },
        executableOperation: async (facts) => {
            const { row } = facts;
            return (await c.query("SELECT * FROM people_directory_operations WHERE id=$1 AND status IN ('queued','retry','running')", [row.id])).rows[0] ?? null;
        },
        provisioning: (facts) => people.provisioning(c, facts),
        currentPlacement: (facts) => people.currentPlacement(c, facts),
        stateForExecution: async (facts) => {
            const { conn } = facts;
            return (await c.query("SELECT * FROM people_directory_state WHERE connection_id=$1", [conn.id])).rows[0] ?? null;
        },
        executionAuthority: async (facts) => {
            const { job, observedNow } = facts;
            return (await c.query("SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND iam_assignment_active(a.id,$4) AND iam_role_allows(a.role,a.company_id,$5)", [
                job.assignment_id,
                job.created_by,
                job.company_id,
                observedNow,
                job.approval_mode === "direct"
                    ? "iam.directory.apply"
                    : "iam.directory.manage",
            ])).rowCount;
        },
        currentDirectAuthority: async (facts) => {
            const { companyId, job, observedNow } = facts;
            return (await c.query("SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.directory.apply')", [companyId, job.created_by, observedNow])).rowCount;
        },
        approvedReview: async (facts) => {
            const { job } = facts;
            return (await c.query("SELECT reviewed_by FROM iam_reviews WHERE id=$1 AND status='approved'", [job.review_id])).rows[0] ?? null;
        },
        currentReviewAuthority: async (facts) => {
            const { companyId, review, observedNow } = facts;
            return (await c.query("SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.review') AND iam_role_allows(a.role,a.company_id,'iam.directory.manage')", [companyId, review.reviewed_by, observedNow])).rowCount;
        },
        markRunning: async (facts) => {
            const { observedNow, job } = facts;
            return void await c.query("UPDATE people_directory_operations SET status='running',attempts=attempts+1,version=version+1,updated_at=$1 WHERE id=$2", [observedNow, job.id]);
        },
        membershipSources: (facts) => organization.membershipSources(c, facts),
        provisioningSources: (facts) => people.provisioningSources(c, facts),
        completeOperation: async (facts) => {
            const { result, observedNow, job } = facts;
            return void await c.query("UPDATE people_directory_operations SET status='completed',result=$1,last_error=NULL,version=version+1,updated_at=$2 WHERE id=$3", [JSON.stringify(result), observedNow, job.id]);
        },
        completionProvisioning: (facts) => people.completionProvisioning(c, facts),
        recordPackageSource: async (facts) => {
            const { conn, action, member, applicationId, observedNow } = facts;
            return void await c.query("INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,$4,$4,$5) ON CONFLICT DO NOTHING", [
                conn.id,
                action.objectId,
                member,
                applicationId,
                observedNow,
            ]);
        },
        recordDirectSource: async (facts) => {
            const { conn, action, member, job, observedNow } = facts;
            return void await c.query("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING", [
                conn.id,
                action.objectId,
                member,
                "direct:" + job.id,
                observedNow,
            ]);
        },
        recordVerificationAudit: async (facts) => {
            const { generatedId, job, result, observedNow } = facts;
            return void await c.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
                generatedId,
                job.company_id,
                job.created_by,
                job.assignment_id,
                "directory.operation.verified",
                job.id,
                JSON.stringify({ result }),
                observedNow,
            ]);
        },
        recordVerificationEvent: async (facts) => {
            const { job, result, observedNow } = facts;
            return recordDirectoryVerification(c,job,result,observedNow);
        },
        failOperation: async (facts) => {
            const { retry, code, observedNow, row } = facts;
            return void await c.query("UPDATE people_directory_operations SET status=$1,last_error=$2,next_attempt_at=$3+LEAST(300000,1000*power(2,LEAST(attempts,8))),version=version+1,updated_at=$3 WHERE id=$4 AND status NOT IN ('completed','cancelled','rejected')", [retry ? "retry" : "blocked", code, observedNow, row.id]);
        }, recordCompletedStep: async ({ job, conn, action, steps, replacedSourceIds, withdrawsSources }) => {
            if (withdrawsSources) {
                await c.query('BEGIN');
                if (action.kind !== 'group.members')
                    throw new Error('Invalid source retirement');
                await c.query('DELETE FROM people_directory_membership_sources WHERE connection_id=$1 AND group_id=$2 AND member_id=ANY($3::text[]) AND application_id=ANY($4::text[])', [conn.id, action.objectId, action.memberIds, replacedSourceIds]);
            }
            await c.query('UPDATE people_directory_operations SET steps=$2 WHERE id=$1', [job.id, JSON.stringify(steps)]);
            if (withdrawsSources)
                await c.query('COMMIT');
        }, finalize: async (run) => { await c.query('BEGIN'); const result = await run(); await c.query('COMMIT'); return result; } };
    clients.set(tx, c);
    return tx;
}
function operationService(raw?: PostgresDatabase) {
    const db = raw ? iamDb(raw) : undefined;
    return createDirectoryOperations({ error: (...args) => new ApiError(...args), now: Date.now, id: randomUUID, hash, failureCode: (error) => error instanceof ApiError ? error.code : 'DIRECTORY_UNAVAILABLE', companyScope: assertCompanyScope, authorize: requirePermission, fresh: requireFreshMfa, inventory: readAutomationInventory, validateTarget: validateObjectTarget, apply: applyDirectoryAction, command: (actor, key, operation, payload, permission, run) => iamCommand(db!, actor, key, operation, payload, permission, (c) => run(operationTx(c))), review: (tx, actor, kind, payload, reason) => proposeIamReview(clients.get(tx)!, actor, kind, payload, reason), snapshot: async (run) => {
            const c = await db!.pool.connect();
            try {
                await c.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
                return await run(operationTx(c));
            }
            finally {
                await c.query('ROLLBACK');
                c.release();
            }
        }, automationConnections: async (facts) => {
            const { actor } = facts;
            return (await db!.pool.query("SELECT c.*,s.version state_version,s.tested_version automation_tested,s.write_enabled,s.inventory,s.definition FROM iam_connections c LEFT JOIN people_directory_state s ON s.connection_id=c.id WHERE c.company_id=$1 AND c.status IN ('draft','tested','active') AND c.provider IN ('univention','active_directory') ORDER BY c.name", [actor.companyId])).rows;
        },
        automationJobs: async (facts) => {
            const { actor } = facts;
            return (await db!.pool.query("SELECT id,connection_id,status,version,definition,last_error,result,review_id,created_at,approval_mode,steps FROM people_directory_operations WHERE company_id=$1 ORDER BY created_at DESC LIMIT 200", [actor.companyId])).rows;
        },
        lifecycleCases: (facts) => people.lifecycleCases(db!.pool, facts),
        dueOperations: async (facts) => {
            const { observedNow, limit } = facts;
            return (await db!.pool.query("SELECT id,connection_id FROM people_directory_operations WHERE status IN ('queued','retry','running') AND next_attempt_at<=$1 ORDER BY created_at LIMIT $2", [observedNow, limit])).rows;
        }, withConnectionLock: async (row, run, failed) => {
            const c = await db!.pool.connect();
            let locked = false;
            try {
                locked = (await c.query('SELECT pg_try_advisory_lock(hashtext($1)) locked', ['people-directory:' + row.connection_id])).rows[0].locked;
                if (!locked)
                    return;
                await run(operationTx(c));
            }
            catch (error) {
                await c.query('ROLLBACK');
                await failed(operationTx(c), error);
            }
            finally {
                if (locked)
                    await c.query('SELECT pg_advisory_unlock(hashtext($1))', ['people-directory:' + row.connection_id]);
                c.release();
            }
        } });
}
