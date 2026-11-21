import { afterEach, describe, expect, it } from 'vitest';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { atomicMigrationJson, MigrationWorkStore, migrationLeaseMs, migrationPathsOverlap, replaceMigrationPlan, runMigrationWorkCli, validateMigrationLedger, type MigrationLedger, type MigrationTask } from '../../../scripts/testing/rust-migration-work';
import { pinnedBunBinary } from '../../../scripts/testing/runtime-tooling';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
const candidate = 'a'.repeat(40), pushed = 'b'.repeat(40);
const passed = { command: 'cargo check -p owned && targeted-owner-regressions', result: 'passed', sourceSha: candidate };
const retired = { command: 'retirement:ts-writes', before: 4, after: 0, result: 'passed', sourceSha: candidate };
function task(id: string, patch: Partial<MigrationTask> = {}): MigrationTask {
  return { id, goalId: 'G1', owner: 'files', workflowIds: [], kind: 'migration', title: id,
    dependsOn: [], requiredPorts: [], allowedPaths: [`packages/${id}/**`], resources: [], status: 'pending',
    claim: null, baseSha: null, implementedCommit: null, lastPushedSha: null, checks: [], retainedLegacy: [], blocker: null,
    nextAction: `Implement ${id}`, handoff: null, ...patch };
}
function fixture(tasks = [task('one'), task('two')], options: ConstructorParameters<typeof MigrationWorkStore>[1] = {}) {
  const root = mkdtempSync(join(tmpdir(), 'erpx-migration-work-')); roots.push(root);
  const ledger: MigrationLedger = {
    version: 1, counts: { workflowsVerified: 78, workflowsMerged: 78, ownersComplete: 8 }, acceptance: false,
    evidencePolicy: { protectedHistory: 'retain' },
    executionPlan: { schemaVersion: 2, revision: 0, targetBranch: 'codex/rust-backend-migration', planRef: 'docs/production-plan.md',
      coordinator: null, goals: [{ id: 'G1' }], tasks, externalReservations: [], migrationProgress: { acceptance: false },
      integrationLog: [{ commit: 'historical', evidence: 'retain verbatim' }] },
  };
  const directory = join(root, 'docs/status'); mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, 'rust-delivery-bundles.json'), `${JSON.stringify(ledger, null, 2)}\n`);
  return { root, ledger, store: new MigrationWorkStore(root, options) };
}
async function begin(store: MigrationWorkStore) { return store.begin({ runId: 'lead-1', agentId: 'coordinator' }); }
async function claim(store: MigrationWorkStore, id = 'one') { return store.claim(id, { runId: 'lead-1', agentId: `worker-${id}` }); }
function cliProcess(root: string, args: string[]) {
  return new Promise<{ code: number | null; stdout: string; stderr: string }>((resolveResult, reject) => {
    const child = spawn(pinnedBunBinary(resolve('.')), [resolve('scripts/testing/rust-migration-work.ts'), ...args, `--root=${root}`],
      { env: { PATH: process.env.PATH, NODE_ENV: 'test' }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    child.stdout.on('data', chunk => { stdout += chunk; }); child.stderr.on('data', chunk => { stderr += chunk; });
    child.on('error', reject); child.on('close', code => resolveResult({ code, stdout, stderr }));
  });
}

describe('portable Rust migration work coordination', () => {
  it('reads status/validates without creating locks, tokens or changing certification', async () => {
    const f = fixture(), before = readFileSync(f.store.ledgerPath, 'utf8'), output: string[] = [];
    await runMigrationWorkCli(['status', '--json', `--root=${f.root}`], text => output.push(text));
    await runMigrationWorkCli(['validate', `--root=${f.root}`], text => output.push(text));
    expect(JSON.parse(output[0])).toMatchObject({ revision: 0, coordinatorStale: false, heartbeatEveryMinutes: 10 });
    expect(readFileSync(f.store.ledgerPath, 'utf8')).toBe(before);
    expect(existsSync(join(f.root, '.scratch'))).toBe(false);
  });

  it('keeps default status compact and exposes exact task/history only by explicit selection', async () => {
    const f = fixture(), ledger = f.store.read();
    ledger.executionPlan.integrationLog = [{ evidence: 'historical-payload'.repeat(10_000) }];
    atomicMigrationJson(f.store.ledgerPath, ledger);
    const compact = f.store.status();
    expect(compact).not.toHaveProperty('integrationLog'); expect(compact).not.toHaveProperty('tasks');
    expect(JSON.stringify(compact).length).toBeLessThan(3000);
    const output: string[] = [];
    await runMigrationWorkCli(['status', '--task=one', `--root=${f.root}`], text => output.push(text));
    expect(JSON.parse(output[0]).task).toEqual(ledger.executionPlan.tasks[0]);
    expect(f.store.status({ full: true })).toHaveProperty('integrationLog', ledger.executionPlan.integrationLog);
    await expect(runMigrationWorkCli(['status', '--task=one', '--full', `--root=${f.root}`])).rejects.toThrow('INVALID_ARGUMENT');
    expect(existsSync(join(f.root, '.scratch'))).toBe(false);
  });

  it('allows exactly one coordinator when actual independent CLI processes begin simultaneously', async () => {
    const f = fixture();
    const results = await Promise.all([
      cliProcess(f.root, ['begin', '--run=process-a', '--agent=alpha']),
      cliProcess(f.root, ['begin', '--run=process-b', '--agent=beta']),
    ]);
    expect(results.map(result => result.code).sort()).toEqual([0, 1]);
    expect(results.find(result => result.code === 1)!.stderr).toContain('COORDINATOR_EXISTS_RECOVERY_REQUIRED');
    const plan = f.store.read().executionPlan;
    expect(plan.revision).toBe(1);
    const tokenPath = join(f.store.privateDirectory, `${plan.coordinator!.runId}.token.json`);
    const token = JSON.parse(readFileSync(tokenPath, 'utf8')).token;
    expect(statSync(tokenPath).mode & 0o777).toBe(0o600);
    expect(results.map(result => result.stdout + result.stderr).join('')).not.toContain(token);
    expect(readFileSync(f.store.ledgerPath, 'utf8')).not.toContain(token);
    await expect(f.store.begin({ runId: plan.coordinator!.runId, agentId: 'another-process' })).rejects.toThrow('COORDINATOR_EXISTS');
  });

  it('serializes simultaneous claims for the same task and overlapping owner paths', async () => {
    const f = fixture([task('one', { allowedPaths: ['packages/server-files/**'] }), task('two', { allowedPaths: ['packages/server-files/src/adapters/file.rs'] })]);
    await begin(f.store);
    const other = new MigrationWorkStore(f.root);
    const same = await Promise.allSettled([claim(f.store), claim(other)]);
    expect(same.filter(result => result.status === 'fulfilled')).toHaveLength(1);
    expect(same.filter(result => result.status === 'rejected')).toHaveLength(1);
    await expect(claim(other, 'two')).rejects.toThrow('RESOURCE_ALREADY_CLAIMED');
    expect(f.store.read().executionPlan.tasks.filter(task => task.claim)).toHaveLength(1);
  });

  it('allows independent owner work in parallel while retaining explicitly shared resource exclusion', async () => {
    const f = fixture([task('one'), task('two'), task('three', { resources: ['database'] }), task('four', { resources: ['database'] })]);
    await begin(f.store);
    await Promise.all([claim(f.store, 'one'), claim(new MigrationWorkStore(f.root), 'two')]);
    await claim(f.store, 'three');
    await expect(claim(f.store, 'four')).rejects.toThrow('RESOURCE_ALREADY_CLAIMED');
    expect(f.store.read().executionPlan.tasks.filter(task => task.claim)).toHaveLength(3);
  });

  it('respects directory boundaries and rejects wildcard/path escapes', () => {
    expect(migrationPathsOverlap('packages/server-files/**', 'packages/server-files/src/file.rs')).toBe(true);
    expect(migrationPathsOverlap('Cargo.toml', 'Cargo.toml')).toBe(true);
    expect(migrationPathsOverlap('packages/server-files/**', 'packages/server-files-extra/**')).toBe(false);
    for (const path of ['/absolute/**', '../outside', 'packages/../outside', 'packages/**/file.rs', 'packages/*', 'a\\b'])
      expect(() => migrationPathsOverlap(path, 'packages/server-files/**')).toThrow('INVALID_ALLOWED_PATH');
  });

  it('blocks dependency consumers until completion and reports ports without inventing readiness', async () => {
    const f = fixture([task('one', { kind: 'inventory' }), task('two', { dependsOn: ['one'], requiredPorts: ['files.bind'] })]);
    await begin(f.store);
    await expect(claim(f.store, 'two')).rejects.toThrow('DEPENDENCIES_NOT_READY');
    await claim(f.store);
    await f.store.update('one', { runId: 'lead-1', status: 'done', nextAction: 'Inventory handed off' });
    await claim(f.store, 'two');
    expect(f.store.status().requiredPortsPolicy).toContain('readiness is not inferred');
  });

  it('preserves external WIP reservations while allowing a read-only inventory', async () => {
    const f = fixture([task('one', { allowedPaths: ['packages/server-files/**'] }), task('two', { kind: 'inventory', allowedPaths: ['packages/server-files/**'] })]);
    const ledger = f.store.read();
    ledger.executionPlan.externalReservations.push({ id: 'existing-writer', allowedPaths: ['packages/server-files/src/**'], reason: 'Uncommitted work', status: 'active' });
    atomicMigrationJson(f.store.ledgerPath, ledger);
    await begin(f.store);
    await expect(claim(f.store)).rejects.toThrow('EXTERNAL_RESERVATION');
    await claim(f.store, 'two');
    expect(f.store.read().executionPlan.tasks[1].claim).not.toBeNull();
  });

  it('keeps external WIP protected until a reviewed narrowing or release and preserves its handoff history', async () => {
    const f = fixture([task('one', { allowedPaths: ['packages/server-files/**'] })]), ledger = f.store.read();
    ledger.executionPlan.externalReservations.push({ id: 'external-files', allowedPaths: ['packages/server-files/**'], reason: 'Existing writer owns WIP', status: 'active' });
    atomicMigrationJson(f.store.ledgerPath, ledger); await begin(f.store);
    await expect(claim(f.store)).rejects.toThrow('EXTERNAL_RESERVATION');
    await expect(f.store.reservation('external-files', { runId: 'lead-1', reason: 'Unchecked', wipReviewed: false, release: true })).rejects.toThrow('RESERVATION_REVIEW_REQUIRED');
    await expect(f.store.reservation('external-files', { runId: 'lead-1', reason: 'Broader scope', wipReviewed: true, paths: ['packages/**'] })).rejects.toThrow('RESERVATION_MAY_ONLY_NARROW');
    const cli = (args: string[]) => runMigrationWorkCli([...args, `--root=${f.root}`], () => {});
    writeFileSync(join(f.root, 'narrow.json'), JSON.stringify(['packages/server-files/src/**']));
    await cli(['reservation', 'external-files', '--run=lead-1', '--reason=Remaining source diff reviewed', '--wip-reviewed', '--paths-file=narrow.json']);
    expect(f.store.read().executionPlan.externalReservations[0]).toMatchObject({ allowedPaths: ['packages/server-files/src/**'], status: 'active', handoff: { previousAllowedPaths: ['packages/server-files/**'], previousReason: 'Existing writer owns WIP' } });
    await expect(claim(f.store)).rejects.toThrow('EXTERNAL_RESERVATION');
    await expect(f.store.reservation('external-files', { runId: 'lead-1', reason: 'Cannot reopen released scope', wipReviewed: true, paths: ['packages/server-files/**'] })).rejects.toThrow('RESERVATION_MAY_ONLY_NARROW');
    await cli(['reservation', 'external-files', '--run=lead-1', '--reason=Holder handed off; dirty work preserved', '--wip-reviewed', '--release']);
    await claim(f.store);
    const reservation = f.store.read().executionPlan.externalReservations[0];
    expect(reservation.status).toBe('released'); expect(reservation.history).toHaveLength(2);
    expect(reservation.handoff).toMatchObject({ previousAllowedPaths: ['packages/server-files/src/**'], wipReviewed: true });
    expect(reservation.history![0]).toMatchObject({ previousReason: 'Existing writer owns WIP' });
  });

  it('rejects stale ledger revisions and replaced private tokens without modifying tracked state', async () => {
    const f = fixture(); await begin(f.store);
    const before = readFileSync(f.store.ledgerPath, 'utf8');
    await expect(f.store.claim('one', { runId: 'lead-1', agentId: 'worker', revision: 0 })).rejects.toThrow('STALE_REVISION');
    expect(readFileSync(f.store.ledgerPath, 'utf8')).toBe(before);
    const tokenPath = join(f.store.privateDirectory, 'lead-1.token.json'), credentials = JSON.parse(readFileSync(tokenPath, 'utf8'));
    writeFileSync(tokenPath, JSON.stringify({ ...credentials, token: 'superseded' }));
    await expect(f.store.heartbeat({ runId: 'lead-1' })).rejects.toThrow('STALE_TOKEN');
    expect(readFileSync(f.store.ledgerPath, 'utf8')).toBe(before);
  });

  it('does not steal expired runs or claims, and requires reviewed explicit recovery with preserved handoff', async () => {
    let now = Date.parse('2026-11-10T12:00:00Z');
    const f = fixture(undefined, { now: () => now }); await begin(f.store); await claim(f.store);
    now += migrationLeaseMs + 1;
    expect(f.store.status().coordinatorStale).toBe(true);
    await expect(f.store.begin({ runId: 'next', agentId: 'new-ai' })).rejects.toThrow('RECOVERY_REQUIRED');
    await expect(f.store.heartbeat({ runId: 'lead-1' })).rejects.toThrow('LEASE_EXPIRED_RECOVERY_REQUIRED');
    expect(f.store.read().executionPlan.tasks[0].claim!.runId).toBe('lead-1');
    await expect(f.store.recover({ runId: 'next', agentId: 'new-ai', reason: 'Old chat ended', wipReviewed: false, previousRunStopped: true })).rejects.toThrow('RECOVERY_REVIEW_REQUIRED');
    await f.store.recover({ runId: 'next', agentId: 'new-ai', reason: 'Old chat ended; dirty diff reviewed', wipReviewed: true, previousRunStopped: true });
    const plan = f.store.read().executionPlan;
    expect(plan.coordinator).toMatchObject({ runId: 'next', generation: 2 });
    expect(plan.tasks[0]).toMatchObject({ claim: null, status: 'blocked', nextAction: 'Implement one', handoff: { runId: 'lead-1', previousStatus: 'in_progress', nextAction: 'Implement one' } });
    await expect(claim(f.store)).rejects.toThrow('COORDINATOR_REQUIRED');
    await f.store.claim('one', { runId: 'next', agentId: 'next-worker' });
    expect(f.store.read().executionPlan.tasks[0].claim!.generation).toBe(2);
  });

  it('renews coordinator and task leases together before expiration', async () => {
    let now = Date.parse('2026-11-10T12:00:00Z');
    const f = fixture(undefined, { now: () => now }); await begin(f.store); await claim(f.store);
    now += 10 * 60_000; await f.store.heartbeat({ runId: 'lead-1' });
    const plan = f.store.read().executionPlan;
    expect(plan.coordinator!.heartbeatAt).toBe(new Date(now).toISOString());
    expect(plan.tasks[0].claim!.expiresAt).toBe(new Date(now + migrationLeaseMs).toISOString());
  });

  it('requires handoff before releasing a run and preserves resumable actions on task release', async () => {
    const f = fixture(); await begin(f.store); await claim(f.store);
    await expect(f.store.release({ runId: 'lead-1' })).rejects.toThrow('ACTIVE_TASKS_REQUIRE_HANDOFF');
    await f.store.handoff('one', { runId: 'lead-1', nextAction: 'Run the frozen source regression, then mount the public port' });
    expect(f.store.read().executionPlan.tasks[0].handoff).toMatchObject({ nextAction: 'Run the frozen source regression, then mount the public port' });
    await claim(f.store, 'two'); await f.store.release({ runId: 'lead-1' }, 'two');
    expect(f.store.read().executionPlan.tasks[1]).toMatchObject({ claim: null, status: 'blocked', nextAction: 'Implement two' });
    await f.store.release({ runId: 'lead-1' });
    await f.store.begin({ runId: 'lead-2', agentId: 'different-ai' });
    expect(f.store.read().executionPlan.coordinator!.generation).toBe(2);
  });

  it('requires integration and exact-source passed evidence before migration without promoting acceptance', async () => {
    const f = fixture(), original = structuredClone(f.ledger); await begin(f.store); await claim(f.store);
    await expect(f.store.update('one', { runId: 'lead-1', status: 'migrated', nextAction: 'Done' })).rejects.toThrow('INTEGRATION_REQUIRED');
    await f.store.update('one', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Mount native port' });
    await f.store.handoff('one', { runId: 'lead-1', nextAction: 'Mount native port' });
    await claim(f.store);
    expect(f.store.read().executionPlan.tasks[0].status).toBe('integrating');
    await expect(f.store.update('one', { runId: 'lead-1', status: 'migrated', nextAction: 'Next packet', implementedCommit: candidate, pushedSha: pushed, checks: [{ ...passed, result: 'failed' }] })).rejects.toThrow('MIGRATION_EVIDENCE_REQUIRED');
    expect(f.store.read().executionPlan.tasks[0].checks).toEqual([]);
    await f.store.update('one', { runId: 'lead-1', status: 'migrated', nextAction: 'Next packet', baseSha: 'c'.repeat(40), implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired], retainedLegacy: [{ consumer: 'unrelated-owner', removalTaskId: 'migrate.two', removalCondition: 'Other owner migrates' }] });
    const ledger = f.store.read();
    expect(ledger.executionPlan.migrationProgress).toMatchObject({ totalPackets: 2, migratedPackets: 1, statusCounts: { migrated: 1, pending: 1 }, acceptance: false });
    expect(ledger.executionPlan.tasks[0]).toMatchObject({ claim: null, implementedCommit: candidate, lastPushedSha: pushed });
    expect(ledger.counts).toEqual(original.counts); expect(ledger.acceptance).toBe(false);
    expect(ledger.evidencePolicy).toEqual(original.evidencePolicy);
    expect(ledger.executionPlan.integrationLog).toEqual(original.executionPlan.integrationLog);
  });

  it('blocks migration without a retirement delta and expiring legacy retention', async () => {
    const f = fixture(); await begin(f.store); await claim(f.store);
    await f.store.update('one', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate' });
    await f.store.handoff('one', { runId: 'lead-1', nextAction: 'Integrate' });
    await claim(f.store);
    const base = { runId: 'lead-1', status: 'migrated' as const, nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed };
    await expect(f.store.update('one', { ...base, checks: [passed] })).rejects.toThrow('RETIREMENT_KPI_REQUIRED');
    await expect(f.store.update('one', { ...base, checks: [passed, { ...retired, after: 4 }] })).rejects.toThrow('RETIREMENT_KPI_REQUIRED');
    await expect(f.store.update('one', { ...base, checks: [passed, retired], retainedLegacy: [{ consumer: 'files', removalCondition: 'later' }] })).rejects.toThrow('RETAINED_LEGACY_REMOVAL_TASK_REQUIRED');
    await f.store.update('one', { ...base, checks: [passed, retired], retainedLegacy: [{ consumer: 'files', removalTaskId: 'migrate.two', removalCondition: 'later' }] });
    expect(f.store.read().executionPlan.tasks[0].status).toBe('migrated');
  });

  it('requires every contract cutover layer when the task declares a cutover', async () => {
    const f = fixture([task('one', { cutover: { clientParity: false, routeAuthority: false, presenterParity: false } })]); await begin(f.store); await claim(f.store);
    await f.store.update('one', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate' });
    await f.store.handoff('one', { runId: 'lead-1', nextAction: 'Integrate' });
    await claim(f.store);
    const base = { runId: 'lead-1', status: 'migrated' as const, nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired] };
    await expect(f.store.update('one', base)).rejects.toThrow('CONTRACT_CUTOVER_INCOMPLETE');
    await f.store.update('one', { ...base, cutover: { clientParity: true, routeAuthority: true, presenterParity: true } });
    expect(f.store.read().executionPlan.tasks[0].status).toBe('migrated');
  });

  it('accepts a read-only retirement statement delta only when it reaches zero', async () => {
    const f = fixture(); await begin(f.store); await claim(f.store);
    await f.store.update('one', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate' });
    await f.store.handoff('one', { runId: 'lead-1', nextAction: 'Integrate' });
    await claim(f.store);
    const base = { runId: 'lead-1', status: 'migrated' as const, nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed };
    await expect(f.store.update('one', { ...base, checks: [passed, { command: 'retirement:ts-statements', before: 12, after: 9, result: 'passed', sourceSha: candidate }] })).rejects.toThrow('RETIRE_TASK_REQUIRED');
    await f.store.update('one', { ...base, checks: [passed, { command: 'retirement:ts-statements', before: 12, after: 0, result: 'passed', sourceSha: candidate }] });
    expect(f.store.read().executionPlan.tasks[0].status).toBe('migrated');
  });

  it('binds a retire task when a migrated packet still records owner writers', async () => {
    const f = fixture([task('migrate.files', { owner: 'files' })]); await begin(f.store); await claim(f.store, 'migrate.files');
    await f.store.update('migrate.files', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate' });
    await f.store.handoff('migrate.files', { runId: 'lead-1', nextAction: 'Integrate' });
    await claim(f.store, 'migrate.files');
    const base = { runId: 'lead-1', status: 'migrated' as const, nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed };
    const partial = { command: 'retirement:ts-writes packages/server-files/src', before: 4, after: 2, result: 'passed', sourceSha: candidate };
    await expect(f.store.update('migrate.files', { ...base, checks: [passed, partial] })).rejects.toThrow('RETIRE_TASK_REQUIRED');
    await f.store.addTask(task('retire.files.writers', { owner: 'files' }), { runId: 'lead-1' });
    await f.store.update('migrate.files', { ...base, checks: [passed, partial] });
    expect(f.store.read().executionPlan.tasks.find(task => task.id === 'migrate.files')?.retireTaskId).toBe('retire.files.writers');
  });

  it('requires a retire task to reach zero or classify its exact residual', async () => {
    const f = fixture([task('retire.files.writers', { owner: 'files' })]); await begin(f.store); await claim(f.store, 'retire.files.writers');
    await f.store.update('retire.files.writers', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate' });
    await f.store.handoff('retire.files.writers', { runId: 'lead-1', nextAction: 'Integrate' });
    await claim(f.store, 'retire.files.writers');
    const base = { runId: 'lead-1', status: 'migrated' as const, nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed };
    const partial = { command: 'retirement:ts-writes packages/server-files/src', before: 4, after: 2, result: 'passed', sourceSha: candidate };
    await expect(f.store.update('retire.files.writers', { ...base, checks: [passed, partial] })).rejects.toThrow('RETIREMENT_TARGET_REQUIRED');
    const residual = [{ path: 'packages/server-files/src/adapters/legacy.ts', writes: 2, removalTaskId: 'retire.files.tail', removalCondition: 'Legacy consumer moves to the native port' }];
    await f.store.update('retire.files.writers', { ...base, checks: [passed, { ...partial, residual }] });
    expect(f.store.read().executionPlan.tasks[0].status).toBe('migrated');
  });

  it('refuses a retirement KPI regression on the same owner command', async () => {
    const f = fixture(); await begin(f.store); await claim(f.store);
    const command = 'retirement:ts-writes packages/server-files/src';
    await f.store.update('one', { runId: 'lead-1', status: 'in_progress', nextAction: 'Count', checks: [{ command, before: 4, after: 2, result: 'passed', sourceSha: candidate }] });
    await expect(f.store.update('one', { runId: 'lead-1', status: 'in_progress', nextAction: 'Recount', checks: [{ command, before: 2, after: 3, result: 'passed', sourceSha: candidate }] })).rejects.toThrow('RETIREMENT_KPI_REGRESSION');
    expect(f.store.read().executionPlan.tasks[0].checks).toHaveLength(1);
  });

  it('reports retirement coverage and open retire tasks in status', async () => {
    const f = fixture([
      task('migrate.files', { owner: 'files', status: 'migrated', implementedCommit: candidate, lastPushedSha: pushed,
        checks: [passed, { command: 'retirement:ts-writes packages/server-files/src', before: 4, after: 2, result: 'passed', sourceSha: candidate }] }),
      task('migrate.legal', { owner: 'legal', status: 'migrated', implementedCommit: candidate, lastPushedSha: pushed,
        checks: [passed, { command: 'retirement:ts-writes packages/server-legal/src', before: 4, after: 0, result: 'passed', sourceSha: candidate }] }),
      task('retire.files.writers', { owner: 'files' }),
    ]);
    const status = f.store.status() as { retirement: Record<string, unknown> };
    expect(status.retirement).toMatchObject({ targetValue: 0, migratePackets: 2, retireTasks: 1, openRetireTasks: ['retire.files.writers'], migratedWithoutRetireTask: 0 });
    const open = f.store.read(); open.executionPlan.tasks.splice(2, 1); atomicMigrationJson(f.store.ledgerPath, open);
    expect((f.store.status() as { retirement: Record<string, unknown> }).retirement).toMatchObject({ migratedWithoutRetireTask: 1, migratedWithoutRetireTaskIds: ['migrate.files'] });
  });

  it('requires the global retirement target and no uncovered packet at final integration', async () => {
    const f = fixture([
      task('integration.final', { kind: 'integration' }),
      task('migrate.legacy', { owner: 'files', status: 'migrated', implementedCommit: candidate, lastPushedSha: pushed,
        checks: [passed, { command: 'retirement:ts-writes packages/server-files/src', before: 4, after: 2, result: 'passed', sourceSha: candidate }] }),
    ]);
    await begin(f.store); await f.store.claim('integration.final', { runId: 'lead-1', agentId: 'lead' });
    await f.store.update('integration.final', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Review' });
    await f.store.update('integration.final', { runId: 'lead-1', status: 'integrating', nextAction: 'Review' });
    const base = { runId: 'lead-1', status: 'migrated' as const, nextAction: 'Acceptance later', implementedCommit: candidate, pushedSha: pushed };
    await expect(f.store.update('integration.final', { ...base, checks: [passed, { ...retired, after: 1 }] })).rejects.toThrow('GLOBAL_RETIREMENT_KPI_REQUIRED');
    await expect(f.store.update('integration.final', { ...base, checks: [passed, retired] })).rejects.toThrow('RETIREMENT_TASKS_INCOMPLETE');
    const bound = f.store.read(); bound.executionPlan.tasks.push(task('retire.files.writers', { owner: 'files', status: 'migrated',
      implementedCommit: candidate, lastPushedSha: pushed, checks: [passed, { command: 'retirement:ts-writes packages/server-files/src', before: 4, after: 0, result: 'passed', sourceSha: candidate }] }));
    atomicMigrationJson(f.store.ledgerPath, bound);
    await f.store.update('integration.final', { ...base, checks: [passed, retired] });
    expect(f.store.read().executionPlan.tasks.find(task => task.id === 'integration.final')?.status).toBe('migrated');
  });

  it('retains older and failed checks while gating on the latest command result in the implemented-source cohort', async () => {
    const f = fixture(); await begin(f.store); await claim(f.store);
    await f.store.update('one', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate', checks: [{ ...passed, result: 'failed', sourceSha: 'c'.repeat(40) }] });
    await f.store.update('one', { runId: 'lead-1', status: 'integrating', nextAction: 'Fix failing check', checks: [{ ...passed, result: 'failed' }] });
    await expect(f.store.update('one', { runId: 'lead-1', status: 'migrated', nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed })).rejects.toThrow('MIGRATION_EVIDENCE_REQUIRED');
    await f.store.update('one', { runId: 'lead-1', status: 'migrated', nextAction: 'Next', implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired] });
    expect(f.store.read().executionPlan.tasks[0].checks).toHaveLength(4);
    expect(f.store.read().executionPlan.tasks[0].status).toBe('migrated');
  });

  it('refines bounded pending tasks, adds slices and excludes aggregate packets from derived progress', async () => {
    const f = fixture(); await begin(f.store);
    await f.store.addTask(task('one.slice'), { runId: 'lead-1' });
    await f.store.refine('one', { allowedPaths: ['packages/server-files/src/adapters/one.rs'], dependsOn: ['two', 'one.slice'], aggregate: true, nextAction: 'Use bounded slices' }, { runId: 'lead-1' });
    expect(f.store.status().migrationProgress).toMatchObject({ totalPackets: 2, migratedPackets: 0 });
    await expect(f.store.refine('one', { owner: 'other' }, { runId: 'lead-1' })).rejects.toThrow('IMMUTABLE_TASK_FIELD');
    await expect(f.store.refine('one', { dependsOn: ['absent'] }, { runId: 'lead-1' })).rejects.toThrow('UNKNOWN_DEPENDENCY');
    await claim(f.store, 'two');
    await expect(f.store.refine('two', { title: 'Overwriting live work' }, { runId: 'lead-1' })).rejects.toThrow('REFINEMENT_REQUIRES_UNCLAIMED_TASK');
    await expect(f.store.addTask(task('new', { status: 'migrated' }), { runId: 'lead-1' })).rejects.toThrow('NEW_TASK_MUST_BE_PENDING');
    await expect(claim(f.store)).rejects.toThrow('AGGREGATE_TASK_NOT_CLAIMABLE');
  });

  it('requires explicit scoped path and resource confirmation before claiming migration placeholders', async () => {
    const f = fixture([task('one', { scopeRefinementRequired: true })]); await begin(f.store);
    await expect(claim(f.store)).rejects.toThrow('SCOPE_REFINEMENT_REQUIRED');
    await expect(f.store.refine('one', { scopeRefinementRequired: false }, { runId: 'lead-1' })).rejects.toThrow('SCOPE_PATHS_AND_RESOURCES_REQUIRED');
    await f.store.refine('one', { scopeRefinementRequired: false, allowedPaths: ['packages/server-files/src/adapters/one.rs'], resources: ['owner:files'] }, { runId: 'lead-1' });
    await claim(f.store);
    expect(f.store.read().executionPlan.tasks[0].scopeRefinementRequired).toBe(false);
    for (const field of ['aggregate', 'scopeRefinementRequired']) {
      const invalid = structuredClone(f.ledger); invalid.executionPlan.tasks[0][field] = 'true';
      expect(() => validateMigrationLedger(invalid)).toThrow('INVALID_TASK_FLAGS');
    }
  });

  it('derives durable goal progress from nonaggregate tasks and preserves deferred acceptance goals', async () => {
    const f = fixture([task('one', { kind: 'inventory' }), task('parent', { aggregate: true, dependsOn: ['one'] })]);
    const ledger = f.store.read(); ledger.executionPlan.goals.push({ id: 'A1', status: 'deferred', acceptance: false });
    ledger.executionPlan.tasks.push(task('acceptance.full', { kind: 'acceptance', goalId: 'A1', status: 'deferred' }));
    atomicMigrationJson(f.store.ledgerPath, ledger);
    await begin(f.store); await claim(f.store);
    expect(f.store.read().executionPlan.goals[0].status).toBe('in_progress');
    await f.store.update('one', { runId: 'lead-1', status: 'done', nextAction: 'Inventory complete' });
    expect(f.store.read().executionPlan.goals).toMatchObject([{ id: 'G1', status: 'done' }, { id: 'A1', status: 'deferred', acceptance: false }]);
    expect(f.store.read().executionPlan.tasks[1].status).toBe('done');
  });

  it('covers the actual 41 owners, seven surfaces and 206 workflows while preserving strict source registers', async () => {
    const source = JSON.parse(readFileSync(resolve('docs/status/rust-delivery-bundles.json'), 'utf8')) as MigrationLedger;
    const ownersSource = readFileSync(resolve('docs/status/rust-backend-migration.json'), 'utf8');
    const owners = JSON.parse(ownersSource) as { owners: { id: string }[] };
    const plan = source.executionPlan;
    expect(owners.owners).toHaveLength(41);
    for (const owner of owners.owners) {
      expect(plan.tasks.find(task => task.id === `inventory.${owner.id}`)?.kind).toBe('inventory');
      expect(plan.tasks.find(task => task.id === `migrate.${owner.id}`)?.kind).toBe('migration');
    }
    const surfaceTasks = plan.tasks.filter(task => task.id.startsWith('inventory.surface.'));
    expect(surfaceTasks).toHaveLength(7);
    for (const surface of surfaceTasks) expect(plan.tasks.find(task => task.id === surface.id.replace(/^inventory\./, 'migrate.'))?.kind).toBe('migration');
    const workflows = source.workflows as { id: string }[];
    expect(new Set(workflows.map(workflow => workflow.id)).size).toBe(206);
    expect(new Set(plan.tasks.filter(task => task.kind === 'migration').flatMap(task => task.workflowIds))).toEqual(new Set(workflows.map(workflow => workflow.id)));
    const f = fixture(), clone = structuredClone(source);
    clone.executionPlan.coordinator = null; for (const task of clone.executionPlan.tasks) task.claim = null;
    const extra = task('fixture.audit', { goalId: plan.goals[0].id, kind: 'inventory' }); clone.executionPlan.tasks.push(extra);
    atomicMigrationJson(f.store.ledgerPath, clone);
    await begin(f.store); await f.store.claim(extra.id, { runId: 'lead-1', agentId: 'fixture-worker' });
    await f.store.update(extra.id, { runId: 'lead-1', status: 'done', nextAction: 'Fixture audit complete' });
    const strictBefore = Object.fromEntries(Object.entries(source).filter(([key]) => key !== 'executionPlan'));
    const strictAfter = Object.fromEntries(Object.entries(f.store.read()).filter(([key]) => key !== 'executionPlan'));
    expect(strictAfter).toEqual(strictBefore);
    expect(readFileSync(resolve('docs/status/rust-backend-migration.json'), 'utf8')).toBe(ownersSource);
  });

  it('keeps final integration blocked by future slices and shared ports outside its original dependency list', async () => {
    const f = fixture([task('integration.final', { kind: 'integration' })]); await begin(f.store);
    await f.store.addTask(task('new.slice'), { runId: 'lead-1' });
    await f.store.addTask(task('ports.new', { kind: 'coordination' }), { runId: 'lead-1' });
    await f.store.claim('integration.final', { runId: 'lead-1', agentId: 'lead' });
    await f.store.update('integration.final', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Review all remaining packets' });
    await f.store.update('integration.final', { runId: 'lead-1', status: 'integrating', nextAction: 'Review all remaining packets' });
    await expect(f.store.update('integration.final', { runId: 'lead-1', status: 'migrated', nextAction: 'Acceptance later', implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired] })).rejects.toThrow('FINAL_MIGRATION_TASKS_INCOMPLETE');
    await f.store.claim('new.slice', { runId: 'lead-1', agentId: 'slice-owner' });
    await f.store.update('new.slice', { runId: 'lead-1', status: 'ready_for_integration', nextAction: 'Integrate' });
    await f.store.update('new.slice', { runId: 'lead-1', status: 'integrating', nextAction: 'Run gates' });
    await f.store.update('new.slice', { runId: 'lead-1', status: 'migrated', nextAction: 'Done', implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired] });
    await expect(f.store.update('integration.final', { runId: 'lead-1', status: 'migrated', nextAction: 'Acceptance later', implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired] })).rejects.toThrow('FINAL_MIGRATION_TASKS_INCOMPLETE');
    await f.store.claim('ports.new', { runId: 'lead-1', agentId: 'port-owner' });
    await f.store.update('ports.new', { runId: 'lead-1', status: 'done', nextAction: 'Port ready' });
    await f.store.update('integration.final', { runId: 'lead-1', status: 'migrated', nextAction: 'Acceptance later', implementedCommit: candidate, pushedSha: pushed, checks: [passed, retired] });
    expect(f.store.read().executionPlan.tasks[0].status).toBe('migrated');
  });

  it('preserves original bytes outside executionPlan including compact historical JSON and escaped strings', async () => {
    const f = fixture();
    const source = `{\n  "history":{"escaped":"\\u0041,}\\\"","executionPlan":{"nested":true}},\n  "executionPlan":${JSON.stringify(f.ledger.executionPlan)},\n  "tail" : [1, {"keep":"  spaces  "}]\n}\n`;
    writeFileSync(f.store.ledgerPath, source);
    await begin(f.store);
    const result = readFileSync(f.store.ledgerPath, 'utf8');
    expect(result.slice(0, result.indexOf('  "executionPlan":'))).toBe(source.slice(0, source.indexOf('  "executionPlan":')));
    expect(result.slice(result.lastIndexOf(',\n  "tail"'))).toBe(source.slice(source.lastIndexOf(',\n  "tail"')));
    expect(JSON.parse(result).history).toEqual(JSON.parse(source).history);
    expect(() => replaceMigrationPlan('{"executionPlan":{},"executionPlan":{}}', f.ledger.executionPlan)).toThrow('DUPLICATE_ROOT_KEY');
  });

  it('rejects malformed ledgers, dependency cycles and unsafe transitions before changing the file', async () => {
    const f = fixture();
    const cycle = structuredClone(f.ledger); cycle.executionPlan.tasks[0].dependsOn = ['two']; cycle.executionPlan.tasks[1].dependsOn = ['one'];
    expect(() => validateMigrationLedger(cycle)).toThrow('DEPENDENCY_CYCLE');
    const duplicate = structuredClone(f.ledger); duplicate.executionPlan.tasks.push(task('one'));
    expect(() => validateMigrationLedger(duplicate)).toThrow('INVALID_TASK');
    writeFileSync(f.store.ledgerPath, '{unfinished');
    await expect(begin(f.store)).rejects.toThrow('LEDGER_UNREADABLE');
    expect(readFileSync(f.store.ledgerPath, 'utf8')).toBe('{unfinished');
    expect(existsSync(join(f.store.privateDirectory, 'work-ledger.lock'))).toBe(false);
  });

  it('keeps the original JSON and removes temporary files if serialization fails before atomic rename', () => {
    const f = fixture(), before = readFileSync(f.store.ledgerPath, 'utf8');
    const circular: { self?: unknown } = {}; circular.self = circular;
    expect(() => atomicMigrationJson(f.store.ledgerPath, circular)).toThrow();
    expect(readFileSync(f.store.ledgerPath, 'utf8')).toBe(before);
    expect(readdirSync(join(f.root, 'docs/status'))).toEqual(['rust-delivery-bundles.json']);
  });

  it('fails closed on an occupied mutex rather than removing another process lock', async () => {
    const f = fixture(undefined, { mutexTimeoutMs: 0 });
    const lock = join(f.store.privateDirectory, 'work-ledger.lock'); mkdirSync(lock, { recursive: true });
    writeFileSync(join(lock, 'owner.json'), JSON.stringify({ pid: process.pid, acquiredAt: new Date().toISOString() }));
    const before = readFileSync(f.store.ledgerPath, 'utf8');
    await expect(begin(f.store)).rejects.toThrow('MUTEX_BUSY_REVIEW_LOCK_OWNER');
    expect(existsSync(lock)).toBe(true); expect(readFileSync(f.store.ledgerPath, 'utf8')).toBe(before);
  });

  it('parses explicit CLI evidence, refinement and recovery arguments and rejects unknown flags', async () => {
    const f = fixture(), output: string[] = [], cli = (args: string[]) => runMigrationWorkCli([...args, `--root=${f.root}`], text => output.push(text));
    for (const args of [['unknown'], ['status', '--force'], ['begin', '--run=x', '--run=y', '--agent=z'], ['status', '--json=false'], ['validate', 'extra']])
      await expect(cli(args)).rejects.toThrow('RUST_MIGRATION_WORK_');
    expect(existsSync(join(f.root, '.scratch'))).toBe(false);
    await cli(['begin', '--run=lead-1', '--agent=coordinator']);
    writeFileSync(join(f.root, 'refinement.json'), JSON.stringify({ title: 'Bounded Files packet', nextAction: 'Implement exact read' }));
    await cli(['refine', 'one', '--run=lead-1', '--task-file=refinement.json']);
    writeFileSync(join(f.root, 'new-task.json'), JSON.stringify(task('new')));
    await cli(['add', '--run=lead-1', '--task-file=new-task.json']);
    await cli(['claim', 'one', '--run=lead-1', '--agent=worker']);
    writeFileSync(join(f.root, 'checks.json'), JSON.stringify([passed])); writeFileSync(join(f.root, 'legacy.json'), JSON.stringify([]));
    await cli(['update', 'one', '--run=lead-1', '--status=ready_for_integration', '--next-action=Mount', `--base-sha=${candidate}`, `--implemented-commit=${candidate}`, `--pushed-sha=${pushed}`, '--check-file=checks.json', '--retained-legacy-file=legacy.json']);
    expect(f.store.read().executionPlan.tasks[0]).toMatchObject({ title: 'Bounded Files packet', baseSha: candidate, checks: [passed], retainedLegacy: [] });
    await expect(cli(['recover', '--run=lead-2', '--agent=next', '--reason=Reviewed'])).rejects.toThrow('RECOVERY_REVIEW_REQUIRED');
    await cli(['recover', '--run=lead-2', '--agent=next', '--reason=Reviewed', '--wip-reviewed', '--previous-run-stopped']);
    expect(f.store.read().executionPlan.coordinator!.runId).toBe('lead-2');
  });
});
