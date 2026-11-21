import { createHash, randomBytes } from 'node:crypto';
import { closeSync, existsSync, fsyncSync, mkdirSync, openSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export const migrationLeaseMs = 30 * 60_000;
export const migrationHeartbeatMs = 10 * 60_000;
export const migrationTargetBranch = 'codex/rust-backend-migration';
export type WorkStatus = 'pending' | 'in_progress' | 'ready_for_integration' | 'integrating' | 'migrated' | 'blocked' | 'deferred' | 'done';
export type WorkKind = 'coordination' | 'inventory' | 'migration' | 'integration' | 'acceptance';
export type WorkClaim = { runId: string; agentId: string; claimedAt: string; heartbeatAt: string; expiresAt: string; generation: number };
export type WorkCoordinator = Omit<WorkClaim, 'claimedAt'> & { startedAt: string; tokenHash: string };
export type MigrationTask = {
  id: string; goalId: string; owner: string; workflowIds: string[]; kind: WorkKind; title: string;
  dependsOn: string[]; requiredPorts: string[]; allowedPaths: string[]; resources: string[]; status: WorkStatus;
  claim: WorkClaim | null; baseSha: string | null; implementedCommit: string | null; lastPushedSha: string | null;
  checks: Record<string, unknown>[]; retainedLegacy: unknown[]; blocker: string | null; nextAction: string;
  handoff: Record<string, unknown> | null; [key: string]: unknown;
};
export type MigrationExecutionPlan = {
  schemaVersion: 2; revision: number; targetBranch: string; planRef: string; coordinator: WorkCoordinator | null;
  goals: { id: string; [key: string]: unknown }[]; tasks: MigrationTask[];
  externalReservations: { id: string; allowedPaths: string[]; reason: string; status: 'active' | 'released'; handoff?: Record<string, unknown>; history?: Record<string, unknown>[]; [key: string]: unknown }[];
  [key: string]: unknown;
};
export type MigrationLedger = { executionPlan: MigrationExecutionPlan; [key: string]: unknown };
type Credentials = { runId: string; generation: number; token: string };
type Mutation = { runId: string; revision?: number };
type StoreOptions = { now?: () => number; mutexTimeoutMs?: number };
const statuses = new Set<WorkStatus>(['pending', 'in_progress', 'ready_for_integration', 'integrating', 'migrated', 'blocked', 'deferred', 'done']);
const kinds = new Set<WorkKind>(['coordination', 'inventory', 'migration', 'integration', 'acceptance']);
const terminal = (status: WorkStatus) => status === 'migrated' || status === 'done';
const retirementCheck = (check: Record<string, unknown>) => typeof check.command === 'string'
  && (check.command.startsWith('retirement:ts-writes') || check.command.startsWith('retirement:ts-statements'));
const retireTask = (task: MigrationTask) => task.id.startsWith('retire.');
const retirementCoversOwner = (id: string, owner: string) => id === `retire.${owner}` || id.startsWith(`retire.${owner}.`);
function latestSourceChecks(task: MigrationTask): Map<string, Record<string, unknown>> {
  const latest = new Map<string, Record<string, unknown>>();
  for (const check of task.checks) if (check.sourceSha === task.implementedCommit && typeof check.command === 'string' && check.command.trim()) latest.set(check.command, check);
  return latest;
}
function latestRetirementCheck(task: MigrationTask): Record<string, unknown> | undefined {
  return [...latestSourceChecks(task).values()].filter(retirementCheck).at(-1);
}
const digest = (token: string) => createHash('sha256').update(token).digest('hex');
function fail(code: string): never { throw new Error(`RUST_MIGRATION_WORK_${code}`); }
function identifier(value: unknown): value is string { return typeof value === 'string' && /^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,159}$/.test(value); }
function strings(value: unknown): value is string[] { return Array.isArray(value) && value.every(item => typeof item === 'string' && !!item.trim()); }
function object(value: unknown): value is Record<string, unknown> { return !!value && typeof value === 'object' && !Array.isArray(value); }
function iso(value: unknown): value is string { return typeof value === 'string' && Number.isFinite(Date.parse(value)); }
function sha(value: unknown): value is string | null { return value === null || typeof value === 'string' && /^[a-f0-9]{40}$/.test(value); }
function pathPrefix(path: string): string {
  const prefix = path.replace(/\/\*\*$/, '').replace(/\/$/, '');
  if (!prefix || prefix.startsWith('/') || prefix.includes('\\') || /[*?\0]/.test(prefix) || prefix.split('/').some(part => !part || part === '.' || part === '..')) fail('INVALID_ALLOWED_PATH');
  return prefix;
}
export function migrationPathsOverlap(left: string, right: string): boolean {
  const a = pathPrefix(left), b = pathPrefix(right);
  return a === b || a.startsWith(`${b}/`) || b.startsWith(`${a}/`);
}
function validClaim(value: unknown): value is WorkClaim {
  return object(value) && identifier(value.runId) && identifier(value.agentId) && iso(value.claimedAt)
    && iso(value.heartbeatAt) && iso(value.expiresAt) && Number.isSafeInteger(value.generation) && Number(value.generation) > 0;
}

/** Validate only the execution extension; historical certification/evidence remain opaque. */
export function validateMigrationLedger(input: unknown): asserts input is MigrationLedger {
  if (!object(input) || !object(input.executionPlan)) fail('INVALID_LEDGER');
  const plan = input.executionPlan;
  if (plan.schemaVersion !== 2 || !Number.isSafeInteger(plan.revision) || Number(plan.revision) < 0
    || plan.targetBranch !== migrationTargetBranch || typeof plan.planRef !== 'string' || !plan.planRef.trim()
    || !Array.isArray(plan.goals) || !Array.isArray(plan.tasks) || !Array.isArray(plan.externalReservations)) fail('INVALID_PLAN');
  if (plan.lastCoordinatorGeneration !== undefined && (!Number.isSafeInteger(plan.lastCoordinatorGeneration) || Number(plan.lastCoordinatorGeneration) < 0)) fail('INVALID_GENERATION');
  if (plan.coordinator !== null) {
    const coordinator = plan.coordinator;
    if (!object(coordinator) || !validClaim({ ...coordinator, claimedAt: coordinator.startedAt })
      || typeof coordinator.tokenHash !== 'string' || !/^[a-f0-9]{64}$/.test(coordinator.tokenHash)) fail('INVALID_COORDINATOR');
  }
  const goalIds = new Set<string>();
  for (const goal of plan.goals) {
    if (!object(goal) || !identifier(goal.id) || goalIds.has(goal.id)) fail('INVALID_GOAL');
    goalIds.add(goal.id);
  }
  const tasks = new Map<string, MigrationTask>();
  for (const value of plan.tasks) {
    if (!object(value) || !identifier(value.id) || tasks.has(value.id) || typeof value.goalId !== 'string' || !goalIds.has(value.goalId)
      || !identifier(value.owner) || typeof value.title !== 'string' || !value.title.trim()
      || !strings(value.workflowIds) || !strings(value.dependsOn) || !strings(value.requiredPorts) || !strings(value.allowedPaths) || !strings(value.resources)
      || !kinds.has(value.kind as WorkKind) || !statuses.has(value.status as WorkStatus)
      || !(value.claim === null || validClaim(value.claim)) || !sha(value.baseSha) || !sha(value.implementedCommit) || !sha(value.lastPushedSha)
      || !Array.isArray(value.checks) || !value.checks.every(object) || !Array.isArray(value.retainedLegacy)
      || !(value.blocker === null || typeof value.blocker === 'string') || typeof value.nextAction !== 'string' || !value.nextAction.trim()
      || !(value.handoff === null || object(value.handoff))) fail('INVALID_TASK');
    for (const path of value.allowedPaths) pathPrefix(path);
    const task = value as MigrationTask;
    if ((task.aggregate !== undefined && typeof task.aggregate !== 'boolean')
      || (task.scopeRefinementRequired !== undefined && typeof task.scopeRefinementRequired !== 'boolean')
      || (task.retireTaskId !== undefined && (typeof task.retireTaskId !== 'string' || !identifier(task.retireTaskId)))) fail('INVALID_TASK_FLAGS');
    if (task.aggregate === true && !task.dependsOn.length) fail('AGGREGATE_DEPENDENCIES_REQUIRED');
    if (task.claim && (!plan.coordinator || !object(plan.coordinator) || task.claim.runId !== plan.coordinator.runId || task.claim.generation !== plan.coordinator.generation)) fail('ORPHAN_CLAIM');
    if (task.claim && terminal(task.status)) fail('TERMINAL_TASK_CLAIMED');
    tasks.set(task.id, task);
  }
  for (const task of plan.tasks) if (task.retireTaskId && !tasks.has(task.retireTaskId)) fail('UNKNOWN_RETIRE_TASK');
  const visited = new Set<string>(), visiting = new Set<string>();
  function visit(id: string): void {
    if (visited.has(id)) return;
    if (visiting.has(id)) fail('DEPENDENCY_CYCLE');
    const task = tasks.get(id);
    if (!task) fail('UNKNOWN_DEPENDENCY');
    visiting.add(id);
    for (const dependency of task.dependsOn) visit(dependency);
    visiting.delete(id); visited.add(id);
  }
  for (const id of tasks.keys()) visit(id);
  const reservationIds = new Set<string>();
  for (const reservation of plan.externalReservations) {
    if (!object(reservation) || !identifier(reservation.id) || reservationIds.has(reservation.id) || !strings(reservation.allowedPaths)
      || typeof reservation.reason !== 'string' || !reservation.reason.trim() || !['active', 'released'].includes(String(reservation.status))
      || (reservation.handoff !== undefined && !object(reservation.handoff))
      || (reservation.history !== undefined && (!Array.isArray(reservation.history) || !reservation.history.every(object)))) fail('INVALID_RESERVATION');
    for (const path of reservation.allowedPaths) pathPrefix(path);
    reservationIds.add(reservation.id);
  }
  const claimed = [...tasks.values()].filter(task => task.claim);
  for (let i = 0; i < claimed.length; i++) for (const other of claimed.slice(i + 1)) {
    if (taskConflict(claimed[i], other)) fail('OVERLAPPING_CLAIMS');
  }
}
function taskConflict(left: MigrationTask, right: MigrationTask): boolean {
  return left.resources.some(resource => right.resources.includes(resource))
    || left.kind !== 'inventory' && right.kind !== 'inventory'
      && left.allowedPaths.some(path => right.allowedPaths.some(other => migrationPathsOverlap(path, other)));
}

/** Temp file + fsync + same-directory rename keeps readers on a complete JSON version. */
export function atomicMigrationJson(path: string, value: unknown, mode?: number): void {
  atomicMigrationText(path, `${JSON.stringify(value, null, 2)}\n`, mode);
}
export function atomicMigrationText(path: string, text: string, mode?: number): void {
  const temporary = join(dirname(path), `.${randomBytes(12).toString('hex')}.migration.tmp`);
  try {
    const descriptor = openSync(temporary, 'wx', mode ?? (existsSync(path) ? statSync(path).mode & 0o777 : 0o600));
    try { writeFileSync(descriptor, text); fsyncSync(descriptor); }
    finally { closeSync(descriptor); }
    renameSync(temporary, path);
  } finally { rmSync(temporary, { force: true }); }
}

/** Preserve every byte outside the mutable root executionPlan property. */
export function replaceMigrationPlan(source: string, plan: MigrationExecutionPlan): string {
  JSON.parse(source);
  const whitespace = (index: number) => { while (/\s/.test(source[index] ?? '') && index < source.length) index++; return index; };
  const stringEnd = (index: number) => {
    for (index++; index < source.length; index++) {
      if (source[index] === '\\') index++;
      else if (source[index] === '"') return index + 1;
    }
    return fail('INVALID_LEDGER');
  };
  let index = whitespace(0) + 1, span: { start: number; end: number } | undefined;
  const keys = new Set<string>();
  while (source[index = whitespace(index)] !== '}') {
    const keyEnd = stringEnd(index), key = JSON.parse(source.slice(index, keyEnd)) as string;
    if (keys.has(key)) fail('DUPLICATE_ROOT_KEY'); keys.add(key);
    index = whitespace(keyEnd) + 1;
    const start = index = whitespace(index);
    if (source[index] === '"') index = stringEnd(index);
    else if (source[index] === '{' || source[index] === '[') {
      let depth = 0;
      do {
        const character = source[index];
        if (character === '"') { index = stringEnd(index); continue; }
        if (character === '{' || character === '[') depth++;
        else if (character === '}' || character === ']') depth--;
        index++;
      } while (depth > 0);
    } else while (index < source.length && !/[,}\s]/.test(source[index])) index++;
    if (key === 'executionPlan') span = { start, end: index };
    index = whitespace(index);
    if (source[index] === ',') index++;
  }
  if (!span) fail('INVALID_LEDGER');
  const serialized = JSON.stringify(plan, null, 2).replaceAll('\n', '\n  ');
  return source.slice(0, span.start) + serialized + source.slice(span.end);
}

export class MigrationWorkStore {
  readonly ledgerPath: string;
  readonly privateDirectory: string;
  private readonly now: () => number;
  private readonly mutexTimeoutMs: number;
  constructor(readonly root: string, options: StoreOptions = {}) {
    this.ledgerPath = join(resolve(root), 'docs/status/rust-delivery-bundles.json');
    this.privateDirectory = join(resolve(root), '.scratch/dev/rust-migration');
    this.now = options.now ?? Date.now;
    this.mutexTimeoutMs = options.mutexTimeoutMs ?? 2000;
  }
  read(): MigrationLedger {
    let input: unknown;
    try { input = JSON.parse(readFileSync(this.ledgerPath, 'utf8')); } catch { fail('LEDGER_UNREADABLE'); }
    validateMigrationLedger(input); return input;
  }
  status(options: { taskId?: string; full?: boolean } = {}): Record<string, unknown> {
    const plan = this.read().executionPlan, coordinator = plan.coordinator;
    const common = { schemaVersion: plan.schemaVersion, revision: plan.revision, targetBranch: plan.targetBranch, planRef: plan.planRef,
      coordinator: coordinator ? { runId: coordinator.runId, agentId: coordinator.agentId, generation: coordinator.generation, heartbeatAt: coordinator.heartbeatAt, expiresAt: coordinator.expiresAt } : null,
      coordinatorStale: !!coordinator && Date.parse(coordinator.expiresAt) <= this.now(), heartbeatEveryMinutes: migrationHeartbeatMs / 60_000,
      requiredPortsPolicy: 'Informational; readiness is not inferred. Dependencies must be explicitly completed.' };
    if (options.taskId) return { ...common, task: this.task(plan, options.taskId) };
    if (options.full) return { ...plan, ...common };
    const active = plan.tasks.filter(task => task.claim), readyTasks: unknown[] = [], blockedTasks: unknown[] = [];
    const summary = (task: MigrationTask) => ({ id: task.id, goalId: task.goalId, owner: task.owner, kind: task.kind, status: task.status, nextAction: task.nextAction, ...(task.claim ? { claim: task.claim } : {}) });
    for (const task of plan.tasks) {
      if (task.claim || terminal(task.status) || task.status === 'deferred' || task.aggregate === true) continue;
      const blockedBy: string[] = task.dependsOn.filter(id => !terminal(this.task(plan, id).status)).map(id => `dependency:${id}`);
      if (task.scopeRefinementRequired === true && ['migration', 'integration'].includes(task.kind)) blockedBy.push('scope-refinement-required');
      for (const other of active) if (taskConflict(task, other)) blockedBy.push(`claimed:${other.id}`);
      if (['migration', 'integration'].includes(task.kind)) for (const reservation of plan.externalReservations) {
        if (reservation.status === 'active' && task.allowedPaths.some(path => reservation.allowedPaths.some(other => migrationPathsOverlap(path, other)))) blockedBy.push(`reservation:${reservation.id}`);
      }
      if (task.status === 'blocked' && task.blocker) blockedBy.push(task.blocker);
      if (blockedBy.length) blockedTasks.push({ id: task.id, blockedBy }); else readyTasks.push(summary(task));
    }
    const migratePackets = plan.tasks.filter(task => task.kind === 'migration' && task.id.startsWith('migrate.'));
    const retireTasks = plan.tasks.filter(retireTask);
    const uncovered = migratePackets.filter(task => task.status === 'migrated' && !task.retireTaskId
      && !retireTasks.some(candidate => retirementCoversOwner(candidate.id, task.owner))
      && !(latestRetirementCheck(task)?.result === 'passed' && Number(latestRetirementCheck(task)?.after) === 0));
    const targetValue = Number(object(plan.migrationGate) && object(plan.migrationGate.retirementKpi) ? plan.migrationGate.retirementKpi.targetValue : 0);
    return { ...common, counts: Object.fromEntries([...statuses].map(status => [status, plan.tasks.filter(task => task.status === status).length])),
      migrationProgress: plan.migrationProgress,
      retirement: { targetValue: Number.isSafeInteger(targetValue) ? targetValue : 0, migratePackets: migratePackets.length,
        retireTasks: retireTasks.length, openRetireTasks: retireTasks.filter(task => !terminal(task.status)).map(task => task.id),
        migratedWithoutRetireTask: uncovered.length, migratedWithoutRetireTaskIds: uncovered.slice(0, 50).map(task => task.id) },
      goals: plan.goals.map(goal => ({ id: goal.id, title: goal.title, status: goal.status, tasks: plan.tasks.filter(task => task.goalId === goal.id).length })),
      externalReservations: plan.externalReservations.map(reservation => ({ id: reservation.id, status: reservation.status, reason: reservation.reason, pathCount: reservation.allowedPaths.length })),
      readyTasks, activeTasks: active.map(summary), blockedTasks, deferredTaskIds: plan.tasks.filter(task => task.status === 'deferred').map(task => task.id) };
  }
  private credentialPath(runId: string) { if (!identifier(runId)) fail('INVALID_RUN_ID'); return join(this.privateDirectory, `${runId}.token.json`); }
  private time() { return new Date(this.now()).toISOString(); }
  private expiry() { return new Date(this.now() + migrationLeaseMs).toISOString(); }
  private async locked<T>(operation: () => T): Promise<T> {
    mkdirSync(this.privateDirectory, { recursive: true, mode: 0o700 });
    const lock = join(this.privateDirectory, 'work-ledger.lock'), deadline = Date.now() + this.mutexTimeoutMs;
    for (;;) {
      try { mkdirSync(lock, { mode: 0o700 }); break; }
      catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
        if (Date.now() >= deadline) fail('MUTEX_BUSY_REVIEW_LOCK_OWNER');
        await delay(10);
      }
    }
    try { writeFileSync(join(lock, 'owner.json'), JSON.stringify({ pid: process.pid, acquiredAt: this.time() }), { mode: 0o600 }); return operation(); }
    finally { rmSync(lock, { recursive: true, force: true }); }
  }
  private revision(plan: MigrationExecutionPlan, expected?: number) {
    if (expected !== undefined && plan.revision !== expected) fail('STALE_REVISION');
  }
  private authorize(plan: MigrationExecutionPlan, runId: string) {
    const coordinator = plan.coordinator;
    if (!coordinator || coordinator.runId !== runId) fail('COORDINATOR_REQUIRED');
    let credential: Credentials;
    try { credential = JSON.parse(readFileSync(this.credentialPath(runId), 'utf8')) as Credentials; } catch { fail('PRIVATE_TOKEN_REQUIRED'); }
    if (credential.runId !== runId || credential.generation !== coordinator.generation || typeof credential.token !== 'string' || digest(credential.token) !== coordinator.tokenHash) fail('STALE_TOKEN');
    if (Date.parse(coordinator.expiresAt) <= this.now()) fail('LEASE_EXPIRED_RECOVERY_REQUIRED');
    return coordinator;
  }
  private save(ledger: MigrationLedger) {
    validateMigrationLedger(ledger);
    const plan = ledger.executionPlan;
    plan.revision++;
    plan.updatedAt = this.time();
    for (let pass = 0; pass < plan.tasks.length; pass++) for (const task of plan.tasks) if (task.aggregate === true && task.dependsOn.length) {
      const members = task.dependsOn.map(id => this.task(plan, id));
      task.status = members.every(member => terminal(member.status)) ? 'done'
        : members.some(member => member.status === 'blocked') ? 'blocked'
          : members.some(member => !['pending', 'deferred'].includes(member.status)) ? 'in_progress' : 'pending';
    }
    const tasks = plan.tasks.filter(task => task.kind === 'migration' && task.aggregate !== true);
    const statusCounts = Object.fromEntries([...statuses].map(status => [status, tasks.filter(task => task.status === status).length]));
    plan.migrationProgress = { ...(object(plan.migrationProgress) ? plan.migrationProgress : {}), totalPackets: tasks.length, migratedPackets: statusCounts.migrated, statusCounts };
    for (const goal of plan.goals) {
      const members = plan.tasks.filter(task => task.goalId === goal.id && task.aggregate !== true);
      goal.status = !members.length ? 'pending' : members.every(task => terminal(task.status)) ? 'done'
        : members.every(task => task.status === 'deferred') ? 'deferred'
          : members.some(task => ['in_progress', 'ready_for_integration', 'integrating', 'blocked'].includes(task.status)) ? 'in_progress' : 'pending';
    }
    validateMigrationLedger(ledger);
    atomicMigrationText(this.ledgerPath, replaceMigrationPlan(readFileSync(this.ledgerPath, 'utf8'), plan));
  }
  private task(plan: MigrationExecutionPlan, id: string) { const task = plan.tasks.find(task => task.id === id); if (!task) fail('UNKNOWN_TASK'); return task; }
  private heldTask(plan: MigrationExecutionPlan, id: string, runId: string) {
    const task = this.task(plan, id);
    if (!task.claim || task.claim.runId !== runId || task.claim.generation !== plan.coordinator!.generation) fail('TASK_CLAIM_REQUIRED');
    return task;
  }
  private async mutate<T>(input: Mutation, operation: (plan: MigrationExecutionPlan) => T): Promise<T> {
    return this.locked(() => {
      const ledger = this.read(), plan = ledger.executionPlan;
      this.revision(plan, input.revision); this.authorize(plan, input.runId);
      const result = operation(plan); this.save(ledger); return result;
    });
  }
  async begin(input: Mutation & { agentId: string }) {
    if (!identifier(input.runId) || !identifier(input.agentId)) fail('INVALID_IDENTITY');
    return this.locked(() => {
      const ledger = this.read(), plan = ledger.executionPlan;
      this.revision(plan, input.revision);
      if (plan.coordinator) fail('COORDINATOR_EXISTS_RECOVERY_REQUIRED');
      const generation = Number(plan.lastCoordinatorGeneration ?? 0) + 1;
      this.startCoordinator(plan, input.runId, input.agentId, generation);
      this.save(ledger); return { runId: input.runId, generation, revision: plan.revision };
    });
  }
  private startCoordinator(plan: MigrationExecutionPlan, runId: string, agentId: string, generation: number) {
    const token = randomBytes(32).toString('hex'), now = this.time();
    atomicMigrationJson(this.credentialPath(runId), { runId, generation, token }, 0o600);
    plan.coordinator = { runId, agentId, startedAt: now, heartbeatAt: now, expiresAt: this.expiry(), generation, tokenHash: digest(token) };
    plan.lastCoordinatorGeneration = generation;
  }
  async claim(id: string, input: Mutation & { agentId: string }) {
    if (!identifier(input.agentId)) fail('INVALID_IDENTITY');
    return this.mutate(input, plan => {
      const task = this.task(plan, id);
      if (task.claim) fail('TASK_ALREADY_CLAIMED');
      if (task.aggregate === true) fail('AGGREGATE_TASK_NOT_CLAIMABLE');
      if (task.scopeRefinementRequired === true && ['migration', 'integration'].includes(task.kind)) fail('SCOPE_REFINEMENT_REQUIRED');
      if (terminal(task.status) || task.status === 'deferred') fail('TASK_NOT_CLAIMABLE');
      if (task.dependsOn.some(id => !terminal(this.task(plan, id).status))) fail('DEPENDENCIES_NOT_READY');
      if (plan.tasks.some(other => other.claim && taskConflict(task, other))) fail('RESOURCE_ALREADY_CLAIMED');
      if (['migration', 'integration'].includes(task.kind) && plan.externalReservations.some(reservation => reservation.status === 'active'
        && task.allowedPaths.some(path => reservation.allowedPaths.some(other => migrationPathsOverlap(path, other))))) fail('EXTERNAL_RESERVATION');
      const now = this.time();
      task.claim = { runId: input.runId, agentId: input.agentId, claimedAt: now, heartbeatAt: now, expiresAt: this.expiry(), generation: plan.coordinator!.generation };
      task.status = task.status === 'ready_for_integration' ? 'integrating' : 'in_progress';
      task.blocker = null;
      return { taskId: id, status: task.status };
    });
  }
  async heartbeat(input: Mutation) {
    return this.mutate(input, plan => {
      const now = this.time(), expiry = this.expiry();
      plan.coordinator!.heartbeatAt = now; plan.coordinator!.expiresAt = expiry;
      for (const task of plan.tasks) if (task.claim?.runId === input.runId) { task.claim.heartbeatAt = now; task.claim.expiresAt = expiry; }
      return { runId: input.runId, expiresAt: expiry };
    });
  }
  async update(id: string, input: Mutation & { status: WorkStatus; nextAction: string; baseSha?: string; implementedCommit?: string; pushedSha?: string; checks?: Record<string, unknown>[]; retainedLegacy?: unknown[]; cutover?: Record<string, unknown> }) {
    if (!statuses.has(input.status) || !input.nextAction.trim() || (input.implementedCommit !== undefined && !sha(input.implementedCommit))
      || (input.baseSha !== undefined && !sha(input.baseSha)) || (input.pushedSha !== undefined && !sha(input.pushedSha))
      || (input.checks !== undefined && (!Array.isArray(input.checks) || !input.checks.every(object)))
      || (input.retainedLegacy !== undefined && !Array.isArray(input.retainedLegacy))
      || (input.cutover !== undefined && !object(input.cutover))) fail('INVALID_UPDATE');
    return this.mutate(input, plan => {
      const task = this.heldTask(plan, id, input.runId);
      if (id === 'integration.final' && terminal(input.status)
        && plan.tasks.some(other => other.id !== id && other.kind !== 'acceptance' && other.aggregate !== true && !terminal(other.status))) fail('FINAL_MIGRATION_TASKS_INCOMPLETE');
      if (input.status === 'pending') fail('INVALID_TRANSITION');
      if (input.status === 'integrating' && !['ready_for_integration', 'integrating'].includes(task.status)) fail('INTEGRATION_HANDOFF_REQUIRED');
      if (input.status === 'migrated' && (task.status !== 'integrating' || !['migration', 'integration'].includes(task.kind))) fail('INTEGRATION_REQUIRED');
      if (input.status === 'done' && ['migration', 'integration'].includes(task.kind)) fail('MIGRATION_STATUS_REQUIRED');
      task.status = input.status; task.nextAction = input.nextAction;
      if (input.baseSha !== undefined) task.baseSha = input.baseSha;
      if (input.implementedCommit !== undefined) task.implementedCommit = input.implementedCommit;
      if (input.pushedSha !== undefined) task.lastPushedSha = input.pushedSha;
      if (input.checks) {
        const previous = new Map<string, Record<string, unknown>>();
        for (const check of task.checks) if (retirementCheck(check)) previous.set(String(check.command), check);
        for (const check of input.checks) {
          if (!retirementCheck(check) || !Number.isInteger(check.after)) continue;
          const earlier = previous.get(String(check.command));
          if (earlier && Number.isInteger(earlier.after) && Number(check.after) > Number(earlier.after)) fail('RETIREMENT_KPI_REGRESSION');
        }
        task.checks.push(...input.checks);
      }
      if (input.retainedLegacy) task.retainedLegacy = input.retainedLegacy;
      if (input.cutover) task.cutover = { ...(object(task.cutover) ? task.cutover as Record<string, unknown> : {}), ...input.cutover };
      if (task.status === 'migrated') {
        const latest = latestSourceChecks(task);
        if (!task.implementedCommit || !task.lastPushedSha || !latest.size || [...latest.values()].some(check => check.result !== 'passed')) fail('MIGRATION_EVIDENCE_REQUIRED');
        if (['migration', 'integration'].includes(task.kind)) {
          const retirement = [...latest.values()].filter(retirementCheck).at(-1);
          if (!retirement || retirement.result !== 'passed' || !Number.isInteger(retirement.before) || !Number.isInteger(retirement.after) || !(Number(retirement.after) < Number(retirement.before))) fail('RETIREMENT_KPI_REQUIRED');
          if (task.retainedLegacy.some(entry => !object(entry) || typeof entry.removalTaskId !== 'string' || !entry.removalTaskId.trim() || typeof entry.removalCondition !== 'string' || !entry.removalCondition.trim())) fail('RETAINED_LEGACY_REMOVAL_TASK_REQUIRED');
          if (task.cutover !== undefined) {
            const cutover = task.cutover as Record<string, unknown>;
            if (!object(cutover) || cutover.clientParity !== true || cutover.routeAuthority !== true || cutover.presenterParity !== true) fail('CONTRACT_CUTOVER_INCOMPLETE');
          }
          if (retireTask(task)) {
            const residual = Array.isArray(retirement.residual) ? retirement.residual : [];
            const classified = residual.length > 0 && residual.every(entry => object(entry) && typeof entry.path === 'string' && !!entry.path.trim()
              && Number.isSafeInteger(entry.writes) && Number(entry.writes) >= 0 && typeof entry.removalTaskId === 'string' && !!entry.removalTaskId.trim()
              && typeof entry.removalCondition === 'string' && !!entry.removalCondition.trim());
            const residualTotal = classified ? residual.reduce((sum, entry) => sum + Number(entry.writes), 0) : -1;
            if (Number(retirement.after) !== 0 && residualTotal !== Number(retirement.after)) fail('RETIREMENT_TARGET_REQUIRED');
          } else if (task.id === 'integration.final') {
            const target = Number(object(plan.migrationGate) && object(plan.migrationGate.retirementKpi) ? plan.migrationGate.retirementKpi.targetValue : 0);
            if (!Number.isSafeInteger(target) || target < 0 || Number(retirement.after) > target) fail('GLOBAL_RETIREMENT_KPI_REQUIRED');
            const uncovered = plan.tasks.filter(other => other.id !== task.id && other.kind === 'migration' && other.id.startsWith('migrate.')
              && other.status === 'migrated' && !other.retireTaskId
              && !plan.tasks.some(candidate => retireTask(candidate) && retirementCoversOwner(candidate.id, other.owner))
              && !(latestRetirementCheck(other)?.result === 'passed' && Number(latestRetirementCheck(other)?.after) === 0));
            if (uncovered.length) fail('RETIREMENT_TASKS_INCOMPLETE');
          } else if (Number(retirement.after) !== 0) {
            const bound = plan.tasks.filter(other => other.id !== task.id && retireTask(other) && retirementCoversOwner(other.id, task.owner));
            const chosen = bound.find(other => other.id === `retire.${task.owner}`) ?? bound[0];
            if (!chosen) fail('RETIRE_TASK_REQUIRED');
            task.retireTaskId = chosen.id;
          }
        }
      }
      if (terminal(task.status)) task.claim = null;
      return { taskId: id, status: task.status };
    });
  }
  async handoff(id: string, input: Mutation & { nextAction: string }) {
    if (!input.nextAction.trim()) fail('NEXT_ACTION_REQUIRED');
    return this.mutate(input, plan => {
      const task = this.heldTask(plan, id, input.runId);
      this.leaveTask(task, input.nextAction, 'handoff');
      return { taskId: id, status: task.status };
    });
  }
  async addTask(task: MigrationTask, input: Mutation) {
    return this.mutate(input, plan => {
      if (plan.tasks.some(existing => existing.id === task.id)) fail('TASK_EXISTS');
      if (task.status !== 'pending' || task.claim || task.baseSha || task.implementedCommit || task.lastPushedSha || task.checks?.length) fail('NEW_TASK_MUST_BE_PENDING');
      plan.tasks.push(task);
      return { taskId: task.id, status: task.status };
    });
  }
  async refine(id: string, fields: Record<string, unknown>, input: Mutation) {
    const allowed = new Set(['title', 'allowedPaths', 'resources', 'dependsOn', 'requiredPorts', 'scope', 'deliverables', 'nextAction', 'aggregate', 'scopeRefinementRequired', 'cutover']);
    if (!object(fields) || Object.keys(fields).some(key => !allowed.has(key))) fail('IMMUTABLE_TASK_FIELD');
    if (fields.cutover !== undefined && !object(fields.cutover)) fail('INVALID_CUTOVER');
    return this.mutate(input, plan => {
      const task = this.task(plan, id);
      if (task.claim || !['pending', 'blocked'].includes(task.status)) fail('REFINEMENT_REQUIRES_UNCLAIMED_TASK');
      if (task.scopeRefinementRequired === true && fields.scopeRefinementRequired === false
        && (!strings(fields.allowedPaths) || !fields.allowedPaths.length || !strings(fields.resources) || !fields.resources.length)) fail('SCOPE_PATHS_AND_RESOURCES_REQUIRED');
      if (fields.scopeRefinementRequired !== undefined && typeof fields.scopeRefinementRequired !== 'boolean') fail('INVALID_SCOPE_REFINEMENT');
      Object.assign(task, fields);
      if (task.aggregate !== undefined && typeof task.aggregate !== 'boolean') fail('INVALID_AGGREGATE');
      return { taskId: id, status: task.status };
    });
  }
  async reservation(id: string, input: Mutation & { reason: string; wipReviewed: boolean; paths?: string[]; release?: boolean }) {
    if (!input.reason.trim() || !input.wipReviewed || (input.release === true) === (input.paths !== undefined)
      || (input.paths !== undefined && (!strings(input.paths) || !input.paths.length))) fail('RESERVATION_REVIEW_REQUIRED');
    return this.mutate(input, plan => {
      const reservation = plan.externalReservations.find(reservation => reservation.id === id);
      if (!reservation) fail('UNKNOWN_RESERVATION');
      if (reservation.status !== 'active') fail('RESERVATION_NOT_ACTIVE');
      if (input.paths) for (const path of input.paths) {
        const narrowed = pathPrefix(path);
        if (!reservation.allowedPaths.some(previous => { const original = pathPrefix(previous); return narrowed === original || narrowed.startsWith(`${original}/`); })) fail('RESERVATION_MAY_ONLY_NARROW');
      }
      const handoff = { at: this.time(), runId: input.runId, reason: input.reason, wipReviewed: true,
        previousAllowedPaths: [...reservation.allowedPaths], previousReason: reservation.reason, previousStatus: reservation.status };
      reservation.history = [...(reservation.history ?? []), handoff]; reservation.handoff = handoff;
      reservation.reason = input.reason;
      if (input.release) reservation.status = 'released';
      else reservation.allowedPaths = [...input.paths!];
      return { reservationId: id, status: reservation.status, allowedPaths: reservation.allowedPaths };
    });
  }
  private leaveTask(task: MigrationTask, nextAction: string, reason: string) {
    task.handoff = { ...task.handoff, runId: task.claim!.runId, agentId: task.claim!.agentId, at: this.time(), previousStatus: task.status, reason, nextAction };
    task.nextAction = nextAction; task.claim = null;
    if (task.status !== 'ready_for_integration') task.status = 'blocked';
    task.blocker = reason;
  }
  async release(input: Mutation, id?: string) {
    return this.mutate(input, plan => {
      if (id) {
        const task = this.heldTask(plan, id, input.runId);
        this.leaveTask(task, task.nextAction, 'released with work preserved');
      } else {
        if (plan.tasks.some(task => task.claim)) fail('ACTIVE_TASKS_REQUIRE_HANDOFF');
        plan.coordinator = null;
      }
      return { released: id ?? input.runId };
    });
  }
  async recover(input: Mutation & { agentId: string; reason: string; wipReviewed: boolean; previousRunStopped: boolean }) {
    if (!identifier(input.runId) || !identifier(input.agentId) || !input.reason.trim() || !input.wipReviewed || !input.previousRunStopped) fail('RECOVERY_REVIEW_REQUIRED');
    return this.locked(() => {
      const ledger = this.read(), plan = ledger.executionPlan;
      this.revision(plan, input.revision);
      const previous = plan.coordinator;
      if (!previous || previous.runId === input.runId) fail('NEW_RUN_REQUIRED');
      for (const task of plan.tasks) if (task.claim) this.leaveTask(task, task.nextAction, `Recovery: ${input.reason}`);
      this.startCoordinator(plan, input.runId, input.agentId, previous.generation + 1);
      plan.lastRecovery = { previousRunId: previous.runId, runId: input.runId, at: this.time(), reason: input.reason, wipReviewed: true, previousRunStopped: true };
      this.save(ledger); return { runId: input.runId, generation: plan.coordinator!.generation, revision: plan.revision };
    });
  }
}

const commands: Record<string, { flags: string[]; positionals: number[] }> = {
  status: { flags: ['json', 'task', 'full'], positionals: [0] }, validate: { flags: [], positionals: [0] },
  begin: { flags: ['run', 'agent'], positionals: [0] }, claim: { flags: ['run', 'agent'], positionals: [1] },
  heartbeat: { flags: ['run'], positionals: [0] },
  update: { flags: ['run', 'status', 'next-action', 'base-sha', 'implemented-commit', 'pushed-sha', 'check-file', 'retained-legacy-file', 'client-parity', 'route-authority', 'presenter-parity'], positionals: [1] },
  handoff: { flags: ['run', 'next-action'], positionals: [1] }, release: { flags: ['run'], positionals: [0, 1] },
  add: { flags: ['run', 'task-file'], positionals: [0] }, refine: { flags: ['run', 'task-file'], positionals: [1] },
  reservation: { flags: ['run', 'reason', 'wip-reviewed', 'paths-file', 'release'], positionals: [1] },
  recover: { flags: ['run', 'agent', 'reason', 'wip-reviewed', 'previous-run-stopped'], positionals: [0] },
};
export async function runMigrationWorkCli(args: string[], write: (text: string) => void = text => process.stdout.write(text)) {
  const [command, ...rest] = args, definition = commands[command];
  if (!definition) fail('UNKNOWN_COMMAND');
  const flags = new Map<string, string | true>(), positional: string[] = [];
  for (const argument of rest) {
    if (!argument.startsWith('--')) { positional.push(argument); continue; }
    const match = /^--([a-z-]+)(?:=(.*))?$/.exec(argument);
    if (!match || ![...definition.flags, 'root', ...(['status', 'validate'].includes(command) ? [] : ['revision'])].includes(match[1]) || flags.has(match[1])) fail('INVALID_ARGUMENT');
    const boolean = ['json', 'full', 'wip-reviewed', 'previous-run-stopped', 'release', 'client-parity', 'route-authority', 'presenter-parity'].includes(match[1]);
    if (boolean ? match[2] !== undefined : !match[2]) fail('INVALID_ARGUMENT');
    flags.set(match[1], match[2] ?? true);
  }
  if (!definition.positionals.includes(positional.length)) fail('INVALID_ARGUMENT');
  const required = (name: string) => { const value = flags.get(name); if (typeof value !== 'string' || !value.trim()) fail(`ARGUMENT_REQUIRED_${name.toUpperCase().replaceAll('-', '_')}`); return value; };
  let revision: number | undefined;
  if (flags.has('revision')) { const value = required('revision'); if (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) fail('INVALID_REVISION'); revision = Number(value); }
  const root = flags.has('root') ? resolve(required('root')) : resolve(import.meta.dir, '../..');
  const store = new MigrationWorkStore(root);
  const jsonFile = (flag: string): unknown => {
    try { return JSON.parse(readFileSync(resolve(root, required(flag)), 'utf8')); } catch (error) {
      if (error instanceof Error && error.message.startsWith('RUST_MIGRATION_WORK_')) throw error;
      fail('INPUT_JSON_UNREADABLE');
    }
  };
  let result: unknown;
  if (command === 'status') {
    if (flags.has('task') && flags.has('full')) fail('INVALID_ARGUMENT');
    result = store.status({ taskId: flags.get('task') as string | undefined, full: flags.get('full') === true });
  }
  else if (command === 'validate') { store.read(); result = { valid: true }; }
  else {
    const common = { runId: required('run'), revision };
    if (command === 'begin') result = await store.begin({ ...common, agentId: required('agent') });
    else if (command === 'claim') result = await store.claim(positional[0], { ...common, agentId: required('agent') });
    else if (command === 'heartbeat') result = await store.heartbeat(common);
    else if (command === 'update') {
      let checks: Record<string, unknown>[] | undefined;
      if (flags.has('check-file')) {
        const source = jsonFile('check-file');
        const rows = Array.isArray(source) ? source : [source];
        if (!rows.every(object)) fail('INVALID_CHECK_FILE'); checks = rows;
      }
      const retainedLegacy = flags.has('retained-legacy-file') ? jsonFile('retained-legacy-file') : undefined;
      if (retainedLegacy !== undefined && !Array.isArray(retainedLegacy)) fail('INVALID_RETAINED_LEGACY');
      const cutover = ['client-parity', 'route-authority', 'presenter-parity'].some(name => flags.has(name))
        ? { clientParity: flags.has('client-parity'), routeAuthority: flags.has('route-authority'), presenterParity: flags.has('presenter-parity') }
        : undefined;
      result = await store.update(positional[0], { ...common, status: required('status') as WorkStatus, nextAction: required('next-action'), baseSha: flags.get('base-sha') as string | undefined, implementedCommit: flags.get('implemented-commit') as string | undefined, pushedSha: flags.get('pushed-sha') as string | undefined, checks, retainedLegacy, cutover });
    } else if (command === 'handoff') result = await store.handoff(positional[0], { ...common, nextAction: required('next-action') });
    else if (command === 'add') result = await store.addTask(jsonFile('task-file') as MigrationTask, common);
    else if (command === 'refine') {
      const fields = jsonFile('task-file'); if (!object(fields)) fail('INVALID_TASK_REFINEMENT');
      result = await store.refine(positional[0], fields, common);
    }
    else if (command === 'reservation') {
      const paths = flags.has('paths-file') ? jsonFile('paths-file') : undefined;
      if (paths !== undefined && !strings(paths)) fail('INVALID_RESERVATION_PATHS');
      result = await store.reservation(positional[0], { ...common, reason: required('reason'), wipReviewed: flags.get('wip-reviewed') === true, paths, release: flags.get('release') === true });
    }
    else if (command === 'release') result = await store.release(common, positional[0]);
    else result = await store.recover({ ...common, agentId: required('agent'), reason: required('reason'), wipReviewed: flags.get('wip-reviewed') === true, previousRunStopped: flags.get('previous-run-stopped') === true });
  }
  write(`${JSON.stringify(result, null, 2)}\n`);
  return result;
}

if (import.meta.main) runMigrationWorkCli(process.argv.slice(2)).catch(error => {
  process.stderr.write(`${error instanceof Error && error.message.startsWith('RUST_MIGRATION_WORK_') ? error.message : 'RUST_MIGRATION_WORK_IO_FAILED'}\n`);
  process.exitCode = 1;
});
