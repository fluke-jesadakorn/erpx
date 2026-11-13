import 'server-only';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import type { PoolClient } from 'pg';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { IamActor } from '@erpx/server-identity_access/domain/iam-actor';
import { assertCompanyScope } from '@erpx/server-identity_access/adapters/iam-context';
import { createHash } from 'node:crypto';
import { validateObjectTarget } from '@erpx/server-identity_access/public/directory-provider';
import { createDirectoryOperations, type OperationTransaction } from '@erpx/server-identity_access/application/directory-operations';
import type { DirectoryChangeInput } from '@erpx/feature-identity_access/lib/directory-operations';

const exec = promisify(execFile);

function readOnly(): never {
  throw new ApiError(
    503,
    'NATIVE_DIRECTORY_OPERATION_REQUIRED',
    'Directory operation writes use the native command service.',
  );
}

/**
 * Read-only Directory preview engine retained for the People Payroll
 * provisioning transaction, which must evaluate the exact directory impact
 * inside its own unit. Every writer is native; the port stubs below are
 * unreachable from `prepareDirectoryPreview`.
 */
function operationTx(c: PoolClient): OperationTransaction {
  const tx: OperationTransaction = {
    connection: async ({ id, actor }) =>
      (await c.query('SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2', [id, actor.companyId])).rows[0] ?? null,
    recordInventory: readOnly,
    lockState: async ({ row }) =>
      (await c.query('SELECT * FROM people_directory_state WHERE connection_id=$1 FOR UPDATE', [row.id])).rows[0] ?? null,
    ownedAttribute: readOnly,
    configureState: readOnly,
    state: async ({ row }) =>
      (await c.query('SELECT * FROM people_directory_state WHERE connection_id=$1', [row.id])).rows[0] ?? null,
    identityLinks: async ({ row, ids }) =>
      (await c.query(
        "SELECT i.*,c.company_id,c.config FROM iam_identities i JOIN iam_connections c ON c.id=i.connection_id WHERE c.config->>'authority'=$1 AND i.external_id=ANY($2::text[]) ORDER BY c.company_id,i.id",
        [row.config.authority, ids],
      )).rows,
    lifecycle: async ({ action, actor, observedNow }) =>
      (await c.query(
        "SELECT c.*,(people_case_active(c.id,$3) OR c.workflow='placement' AND c.status='completed' AND c.effective_at<=$3 AND EXISTS(SELECT 1 FROM people_applied_packages ap WHERE ap.company_id=c.company_id AND ap.person_id=c.person_id AND people_application_active(ap.id,$3))) active FROM people_lifecycle_cases c WHERE c.id=$1 AND c.company_id=$2",
        [action.caseId, actor.companyId, observedNow],
      )).rows[0] ?? null,
    verifiedLifecycleAccount: async ({ row, action, actor, lifecycle }) =>
      (await c.query(
        "SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE i.connection_id=$1 AND i.external_id=$2 AND m.company_id=$3 AND m.person_id=$4 AND m.status='active'",
        [row.id, action.objectId, actor.companyId, lifecycle.person_id],
      )).rowCount,
    relatedConnections: async ({ row }) =>
      (await c.query("SELECT id,company_id FROM iam_connections WHERE config->>'authority'=$1", [row.config.authority])).rows,
    affectedPositions: async ({ relatedConnections, ids }) =>
      (await c.query(
        `SELECT DISTINCT p.id,p.company_id,p.title FROM organization_positions p
         JOIN people_position_packages b ON b.position_id=p.id AND b.company_id=p.company_id
         JOIN people_package_revisions r ON r.package_id=b.package_id AND r.revision=b.revision AND r.company_id=b.company_id
         WHERE r.definition->>'connectionId'=ANY($1::text[]) AND (EXISTS(SELECT 1 FROM jsonb_array_elements_text(r.definition->'directoryGroupIds') g WHERE g=ANY($2::text[])) OR EXISTS(SELECT 1 FROM jsonb_array_elements(r.definition->'windowsPolicies') w WHERE w->>'id'=ANY($2::text[]))) ORDER BY p.title`,
        [relatedConnections.map((r) => r.id), ids],
      )).rows,
    companyNames: async ({ companyIds }) =>
      (await c.query("SELECT company_id id,display_name name FROM organization_nodes WHERE company_id=ANY($1::text[]) AND kind='company' ORDER BY company_id", [companyIds])).rows,
    protectedAccount: async ({ i, observedNow, actor }) =>
      (await c.query(
        "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 AND a.role='holding_owner' AND iam_assignment_active(a.id,$3)) OR EXISTS(SELECT 1 FROM iam_members m WHERE m.user_id=$1 AND m.company_id=$2 AND m.login_policy='emergency' AND m.status='active') protected,iam_same_person($1,$4,$2) self",
        [i.user_id, i.company_id, observedNow, actor.userId],
      )).rows[0] ?? null,
    affectedPeople: async ({ identities, positionRows }) =>
      (await c.query(
        `SELECT DISTINCT p.legal_name FROM hr_people p LEFT JOIN iam_members m ON m.person_id=p.id AND m.company_id=p.company_id WHERE p.user_id=ANY($1::text[]) OR m.user_id=ANY($1::text[]) OR EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.position_id=ANY($2::text[]) AND e.status='active') ORDER BY p.legal_name`,
        [identities.map((i) => i.user_id).filter(Boolean), positionRows.map((p) => p.id)],
      )).rows,
    previewMembershipClaims: async ({ row, action }) =>
      (await c.query(
        `SELECT s.member_id,s.source_id FROM people_directory_membership_sources s LEFT JOIN people_applied_packages ap ON ap.id=s.application_id WHERE s.connection_id=$1 AND s.group_id=$2 AND s.member_id=ANY($3::text[]) AND s.application_id IS NOT NULL AND ap.ended_at IS NULL`,
        [row.id, action.objectId, action.memberIds],
      )).rows,
    directCompanyAuthority: readOnly,
    insertOperation: readOnly,
    lockPendingOperation: readOnly,
    beneficiaryIsReviewer: readOnly,
    reviewCompanyAuthority: readOnly,
    decideOperation: readOnly,
    resumeOperation: readOnly,
    resumeProvisioning: readOnly,
    cancelOperation: readOnly,
    cancelReview: readOnly,
    executableOperation: readOnly,
    provisioning: readOnly,
    currentPlacement: readOnly,
    stateForExecution: readOnly,
    executionAuthority: readOnly,
    currentDirectAuthority: readOnly,
    approvedReview: readOnly,
    currentReviewAuthority: readOnly,
    markRunning: readOnly,
    membershipSources: readOnly,
    provisioningSources: readOnly,
    completeOperation: readOnly,
    completionProvisioning: readOnly,
    recordPackageSource: readOnly,
    recordDirectSource: readOnly,
    recordVerificationAudit: readOnly,
    recordVerificationEvent: readOnly,
    failOperation: readOnly,
    recordCompletedStep: readOnly,
    finalize: async (run) => run(),
  };
  return tx;
}

function service() {
  return createDirectoryOperations({
    error: (...args) => new ApiError(...args),
    now: Date.now,
    id: () => {
      throw new ApiError(503, 'NATIVE_DIRECTORY_OPERATION_REQUIRED', 'Native identifiers are owned by the Rust service.');
    },
    hash: (value) => createHash('sha256').update(JSON.stringify(value)).digest('hex'),
    failureCode: (error) => (error instanceof ApiError ? error.code : 'DIRECTORY_UNAVAILABLE'),
    companyScope: assertCompanyScope,
    authorize: () => {},
    fresh: async () => {},
    inventory: readOnly,
    validateTarget: validateObjectTarget,
    apply: readOnly,
    command: readOnly,
    snapshot: readOnly,
    review: readOnly,
    withConnectionLock: readOnly,
    automationConnections: async () => [],
    automationJobs: async () => [],
    lifecycleCases: async () => [],
    dueOperations: async () => [],
  });
}

/**
 * Evaluate the exact directory impact inside the caller's transaction. This is
 * a read-only oracle for People Payroll provisioning; every command is native.
 */
export function prepareDirectoryPreview(
  c: PoolClient,
  actor: IamActor,
  input: DirectoryChangeInput,
) {
  return service().prepareDirectoryPreview(operationTx(c), actor, input);
}

/**
 * Run one bounded native Directory operation cycle for every Company that owns
 * directory work in this Holding database. The native worker owns the advisory
 * lock, provider effect and verification; this transport only names the
 * database manifest and Company.
 */
export async function drainDirectoryOperations(db: PostgresDatabase): Promise<void> {
  const binary = process.env.ERPX_NATIVE_SERVER_BIN;
  const manifest = process.env.ERPX_DATABASE_MANIFEST;
  if (!binary || !manifest)
    throw Object.assign(
      new Error('The native directory-operation worker is not configured.'),
      { code: 'NATIVE_DIRECTORY_OPERATION_WORKER_REQUIRED' },
    );
  const companies = await db
    .prepare(
      "SELECT DISTINCT company_id FROM people_directory_operations WHERE status IN ('queued','retry','running','pending_review') ORDER BY company_id",
    )
    .all<{ company_id: string }>();
  for (const row of companies.results ?? []) {
    if (!row.company_id) continue;
    let stdout: string;
    try {
      ({ stdout } = await exec(
        binary,
        [
          'directory-operation-worker',
          '--once',
          '--database-manifest',
          manifest,
          '--company',
          row.company_id,
        ],
        { timeout: 120_000, maxBuffer: 2 * 1024 * 1024, env: process.env, windowsHide: true },
      ));
    } catch {
      throw Object.assign(
        new Error('The native directory-operation worker did not complete.'),
        { code: 'NATIVE_DIRECTORY_OPERATION_WORKER_FAILED' },
      );
    }
    try {
      JSON.parse(stdout);
    } catch {
      throw Object.assign(
        new Error('The native directory-operation worker response is invalid.'),
        { code: 'NATIVE_DIRECTORY_OPERATION_WORKER_RESPONSE_INVALID' },
      );
    }
  }
}
