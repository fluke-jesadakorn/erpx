import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativeLearningPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope:
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativeLearningConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { permissions: string[]; modules: string[] };

export type NativeLearningReadInput = Readonly<{
  holdingSlug: string;
  personal?: boolean;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeLearningCourse = Readonly<{
  id: string;
  company_id: string;
  title: string;
  description: string;
  provider: string;
  duration_minutes: number;
  required_for: string | null;
  status: string;
  created_by: string;
  reviewed_by: string | null;
  reviewed_at: number | null;
  version: number;
  created_at: number;
  updated_at: number;
}>;

export type NativeLearningAssignment = Readonly<{
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  course_id: string;
  course_version: number;
  learner_id: string;
  due_date: string;
  status: string;
  attendance: string | null;
  score_bps: number | null;
  result_evidence: string | null;
  completion_reason: string | null;
  created_by: string;
  version: number;
  created_at: number;
  updated_at: number;
  course_title: string;
  required_for: string | null;
  learner_name: string | null;
  learner_eligible: 0 | 1;
}>;

export type NativeLearningPerson = Readonly<{
  id: string;
  name: string;
  branch_id: string | null;
  department_id: string | null;
}>;

export type NativeLearningEvent = Readonly<{
  id: string;
  assignment_id: string;
  version: number;
  action: string;
  actor_id: string;
  reason: string;
  status: string;
  created_at: number;
}>;

export type NativeLearningHandoff = Readonly<{
  id: string;
  assignment_id: string;
  company_id: string;
  person_id: string;
  eligibility_code: string;
  source_assignment_version: number;
  status: string;
  created_at: number;
}>;

export type NativeLearningCertificate = Readonly<{
  id: string;
  handoff_id: string;
  company_id: string;
  person_id: string;
  assignment_id: string;
  source_assignment_version: number;
  eligibility_code: string;
  valid_from: string;
  expires_on: string | null;
  completion_kind: string;
  evidence_ref: string;
  issued_by: string;
  created_at: number;
  revocation_id: string | null;
}>;

export type NativeLearningEligibilityConsumption = Readonly<{
  id: string;
  election_id: string;
  source_assignment_version: number;
  eligibility_code: string;
  source_digest: string;
  purpose: string;
  actor_id: string;
  command_id: string;
  created_at: number;
}>;

export type NativeLearningEligibilityCertificate = Readonly<{
  id: string;
  handoff_id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  person_id: string;
  assignment_id: string;
  source_assignment_version: number;
  eligibility_code: string;
  valid_from: string;
  expires_on: string | null;
  completion_kind: string;
  evidence_ref: string;
  issued_by: string;
  created_at: number;
  revocation_id: string | null;
  handoff_source_assignment_version: number;
  assignment_version: number;
  assignment_status: string;
  person_status: string;
  person_merged_into_id: string | null;
  current: boolean;
  consumptions: NativeLearningEligibilityConsumption[];
}>;

export type NativeLearningEligibilityFacts = Readonly<{
  person_id: string;
  eligibility_code: string;
  effective_date: string;
  eligible: boolean;
  certificates: NativeLearningEligibilityCertificate[];
}>;

export type NativeLearningWorkspace = Readonly<{
  courses: NativeLearningCourse[];
  assignments: NativeLearningAssignment[];
  people: NativeLearningPerson[];
  canWrite: boolean;
  canReview: boolean;
  actorId: string;
}>;

export type NativeLearningDetail = NativeLearningWorkspace &
  Readonly<{
    assignment: NativeLearningAssignment;
    history: NativeLearningEvent[];
    handoff: NativeLearningHandoff | null;
    certificate: NativeLearningCertificate | null;
  }>;

export type NativeLearningReceipt = Readonly<{
  id: string;
  status: string;
  version: number;
}>;

export type NativeLearningTransport = (
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeLearningError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Learning request could not be completed.');
    this.name = 'NativeLearningError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeLearningError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown) =>
  value === null || (identifier(value) && !/[\u0000-\u001f\u007f]/.test(value));
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  typeof value === 'string' &&
  value.length >= minimum && value.length <= maximum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const content = (value: unknown, minimum: number, maximum: number): value is string =>
  typeof value === 'string' && value.length >= minimum && value.length <= maximum &&
  value.trim() === value && !value.includes('\u0000');
const date = (value: unknown): value is string => {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  if (year < 1900 || year > 2200 || month < 1 || month > 12 || day < 1) return false;
  return day <= new Date(Date.UTC(year, month, 0)).getUTCDate();
};
const optionalDate = (value: unknown) => value === null || date(value);
const digest = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const courseStatuses = new Set(['draft', 'published', 'retired', 'rejected']);
const assignmentStatuses = new Set([
  'assigned',
  'in_progress',
  'completed',
  'waived',
  'cancelled',
]);
const attendanceValues = new Set(['attended', 'absent']);

const course = (value: unknown): value is NativeLearningCourse =>
  exact(value, [
    'id',
    'company_id',
    'title',
    'description',
    'provider',
    'duration_minutes',
    'required_for',
    'status',
    'created_by',
    'reviewed_by',
    'reviewed_at',
    'version',
    'created_at',
    'updated_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  content(value.title, 3, 160) &&
  content(value.description, 3, 3_000) &&
  content(value.provider, 2, 160) &&
  integer(value.duration_minutes, 1) &&
  (value.required_for === null || content(value.required_for, 2, 120)) &&
  typeof value.status === 'string' &&
  courseStatuses.has(value.status) &&
  identifier(value.created_by) &&
  optionalIdentifier(value.reviewed_by) &&
  (value.reviewed_at === null || integer(value.reviewed_at, 1)) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  value.updated_at >= value.created_at &&
  !(
    ['published', 'retired', 'rejected'].includes(value.status as string) &&
    (value.reviewed_by === null || value.reviewed_at === null)
  );

const assignment = (value: unknown): value is NativeLearningAssignment =>
  exact(value, [
    'id',
    'company_id',
    'branch_id',
    'department_id',
    'course_id',
    'course_version',
    'learner_id',
    'due_date',
    'status',
    'attendance',
    'score_bps',
    'result_evidence',
    'completion_reason',
    'created_by',
    'version',
    'created_at',
    'updated_at',
    'course_title',
    'required_for',
    'learner_name',
    'learner_eligible',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  identifier(value.branch_id) &&
  optionalIdentifier(value.department_id) &&
  identifier(value.course_id) &&
  integer(value.course_version, 1) &&
  identifier(value.learner_id) &&
  date(value.due_date) &&
  typeof value.status === 'string' &&
  assignmentStatuses.has(value.status) &&
  (value.attendance === null ||
    (typeof value.attendance === 'string' && attendanceValues.has(value.attendance))) &&
  (value.score_bps === null || (integer(value.score_bps) && value.score_bps <= 10_000)) &&
  (value.result_evidence === null || content(value.result_evidence, 1, 3_000)) &&
  (value.completion_reason === null || content(value.completion_reason, 1, 3_000)) &&
  identifier(value.created_by) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  value.updated_at >= value.created_at &&
  content(value.course_title, 1, 160) &&
  (value.required_for === null || content(value.required_for, 2, 120)) &&
  (value.learner_name === null || trimmed(value.learner_name, 1, 300)) &&
  (value.learner_eligible === 0 || value.learner_eligible === 1);

const person = (value: unknown): value is NativeLearningPerson =>
  exact(value, ['id', 'name', 'branch_id', 'department_id']) &&
  identifier(value.id) &&
  trimmed(value.name, 1, 300) &&
  optionalIdentifier(value.branch_id) &&
  optionalIdentifier(value.department_id);

const event = (value: unknown): value is NativeLearningEvent =>
  exact(value, ['id', 'assignment_id', 'version', 'action', 'actor_id', 'reason', 'status', 'created_at']) &&
  identifier(value.id) &&
  identifier(value.assignment_id) &&
  integer(value.version, 1) &&
  value.version <= 1_000_000 &&
  trimmed(value.action, 1, 200) &&
  identifier(value.actor_id) &&
  content(value.reason, 1, 3_000) &&
  typeof value.status === 'string' &&
  assignmentStatuses.has(value.status) &&
  integer(value.created_at);

const handoff = (value: unknown): value is NativeLearningHandoff =>
  exact(value, [
    'id',
    'assignment_id',
    'company_id',
    'person_id',
    'eligibility_code',
    'source_assignment_version',
    'status',
    'created_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.assignment_id) &&
  identifier(value.company_id) &&
  identifier(value.person_id) &&
  content(value.eligibility_code, 1, 120) &&
  integer(value.source_assignment_version, 1) &&
  value.status === 'open' &&
  integer(value.created_at);

const certificate = (value: unknown): value is NativeLearningCertificate =>
  exact(value, [
    'id',
    'handoff_id',
    'company_id',
    'person_id',
    'assignment_id',
    'source_assignment_version',
    'eligibility_code',
    'valid_from',
    'expires_on',
    'completion_kind',
    'evidence_ref',
    'issued_by',
    'created_at',
    'revocation_id',
  ]) &&
  identifier(value.id) &&
  identifier(value.handoff_id) &&
  identifier(value.company_id) &&
  identifier(value.person_id) &&
  identifier(value.assignment_id) &&
  integer(value.source_assignment_version, 1) &&
  content(value.eligibility_code, 1, 120) &&
  date(value.valid_from) &&
  optionalDate(value.expires_on) &&
  (value.expires_on === null || (value.expires_on as string) >= (value.valid_from as string)) &&
  (value.completion_kind === 'completed' || value.completion_kind === 'waived') &&
  content(value.evidence_ref, 1, 3_000) &&
  identifier(value.issued_by) &&
  integer(value.created_at) &&
  optionalIdentifier(value.revocation_id);

const eligibilityConsumption = (value: unknown): value is NativeLearningEligibilityConsumption =>
  exact(value, [
    'id',
    'election_id',
    'source_assignment_version',
    'eligibility_code',
    'source_digest',
    'purpose',
    'actor_id',
    'command_id',
    'created_at',
  ]) &&
  identifier(value.id) &&
  identifier(value.election_id) &&
  integer(value.source_assignment_version, 1) &&
  content(value.eligibility_code, 1, 120) &&
  digest(value.source_digest) &&
  trimmed(value.purpose, 1, 120) &&
  identifier(value.actor_id) &&
  identifier(value.command_id) &&
  integer(value.created_at);

const eligibilityCertificate = (
  value: unknown,
  actor: NativeLearningPageContext,
  personId: string,
  code: string,
  effectiveDate: string,
): value is NativeLearningEligibilityCertificate => {
  if (
    !exact(value, [
      'id',
      'handoff_id',
      'company_id',
      'branch_id',
      'department_id',
      'person_id',
      'assignment_id',
      'source_assignment_version',
      'eligibility_code',
      'valid_from',
      'expires_on',
      'completion_kind',
      'evidence_ref',
      'issued_by',
      'created_at',
      'revocation_id',
      'handoff_source_assignment_version',
      'assignment_version',
      'assignment_status',
      'person_status',
      'person_merged_into_id',
      'current',
      'consumptions',
    ])
  )
    return false;
  const item = value as Record<string, unknown>;
  if (
    !identifier(item.id) ||
    !identifier(item.handoff_id) ||
    item.company_id !== actor.companyId ||
    (actor.branchId !== null && item.branch_id !== actor.branchId) ||
    (actor.departmentId !== null && item.department_id !== actor.departmentId) ||
    !identifier(item.branch_id) ||
    !optionalIdentifier(item.department_id) ||
    item.person_id !== personId ||
    !identifier(item.assignment_id) ||
    !integer(item.source_assignment_version, 1) ||
    item.eligibility_code !== code ||
    !date(item.valid_from) ||
    !optionalDate(item.expires_on) ||
    (item.expires_on !== null && (item.expires_on as string) < (item.valid_from as string)) ||
    (item.completion_kind !== 'completed' && item.completion_kind !== 'waived') ||
    !content(item.evidence_ref, 1, 3_000) ||
    !identifier(item.issued_by) ||
    !integer(item.created_at) ||
    !optionalIdentifier(item.revocation_id) ||
    !integer(item.handoff_source_assignment_version, 1) ||
    !integer(item.assignment_version, 1) ||
    typeof item.assignment_status !== 'string' ||
    !assignmentStatuses.has(item.assignment_status) ||
    !trimmed(item.person_status, 1, 120) ||
    !optionalIdentifier(item.person_merged_into_id) ||
    !boolean(item.current) ||
    !Array.isArray(item.consumptions) ||
    item.consumptions.length > 100 ||
    !item.consumptions.every(eligibilityConsumption)
  )
    return false;
  const consumptions = item.consumptions as NativeLearningEligibilityConsumption[];
  if (new Set(consumptions.map((row) => row.id)).size !== consumptions.length) return false;
  const current =
    (item.assignment_status === 'completed' || item.assignment_status === 'waived') &&
    item.completion_kind === item.assignment_status &&
    item.handoff_source_assignment_version === item.assignment_version &&
    item.source_assignment_version === item.assignment_version &&
    item.person_status === 'active' &&
    item.person_merged_into_id === null &&
    item.revocation_id === null &&
    (item.valid_from as string) <= effectiveDate &&
    (item.expires_on === null || effectiveDate <= (item.expires_on as string));
  return item.current === current;
};

const eligibilityFacts = (
  value: unknown,
  actor: NativeLearningPageContext,
  personId: string,
  code: string,
  effectiveDate: string,
): NativeLearningEligibilityFacts => {
  if (
    !exact(value, ['person_id', 'eligibility_code', 'effective_date', 'eligible', 'certificates']) ||
    value.person_id !== personId ||
    value.eligibility_code !== code ||
    value.effective_date !== effectiveDate ||
    !boolean(value.eligible) ||
    !Array.isArray(value.certificates) ||
    value.certificates.length < 1 ||
    value.certificates.length > 100 ||
    !value.certificates.every((item) =>
      eligibilityCertificate(item, actor, personId, code, effectiveDate),
    )
  )
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  const certificates = value.certificates as NativeLearningEligibilityCertificate[];
  const ids = new Set<string>();
  for (const item of certificates) {
    if (ids.has(item.id)) return failure('NATIVE_LEARNING_RESPONSE_INVALID');
    ids.add(item.id);
  }
  if (value.eligible !== certificates.some((item) => item.current))
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  return value as unknown as NativeLearningEligibilityFacts;
};

function workspace(
  value: unknown,
  actor: NativeLearningPageContext,
  personal = false,
): NativeLearningWorkspace {
  if (
    !exact(value, ['courses', 'assignments', 'people', 'canWrite', 'canReview', 'actorId']) ||
    !Array.isArray(value.courses) ||
    value.courses.length > 500 ||
    !value.courses.every(course) ||
    !Array.isArray(value.assignments) ||
    value.assignments.length > 1_000 ||
    !value.assignments.every(assignment) ||
    !Array.isArray(value.people) ||
    value.people.length > 500 ||
    !value.people.every(person) ||
    !boolean(value.canWrite) ||
    !boolean(value.canReview) ||
    !identifier(value.actorId) ||
    value.actorId !== actor.userId
  )
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  const courseIds = new Set<string>();
  for (const item of value.courses) {
    if (item.company_id !== actor.companyId || courseIds.has(item.id))
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
    courseIds.add(item.id);
  }
  const assignmentIds = new Set<string>();
  for (const item of value.assignments) {
    if (
      item.company_id !== actor.companyId ||
      (personal && item.learner_id !== actor.userId) ||
      (actor.branchId !== null && item.branch_id !== actor.branchId) ||
      (actor.departmentId !== null && item.department_id !== actor.departmentId) ||
      assignmentIds.has(item.id)
    )
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
    assignmentIds.add(item.id);
  }
  const peopleIds = new Set<string>();
  for (const item of value.people) {
    if (
      (actor.branchId !== null && item.branch_id !== actor.branchId) ||
      (actor.departmentId !== null && item.department_id !== actor.departmentId) ||
      peopleIds.has(item.id)
    )
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
    peopleIds.add(item.id);
  }
  return value as unknown as NativeLearningWorkspace;
}

function detail(
  value: unknown,
  actor: NativeLearningPageContext,
  id: string,
): NativeLearningDetail {
  if (
    !exact(value, [
      'courses',
      'assignments',
      'people',
      'canWrite',
      'canReview',
      'actorId',
      'assignment',
      'history',
      'handoff',
      'certificate',
    ])
  )
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  const base = workspace(
    {
      courses: value.courses,
      assignments: value.assignments,
      people: value.people,
      canWrite: value.canWrite,
      canReview: value.canReview,
      actorId: value.actorId,
    },
    actor,
  );
  if (
    !assignment(value.assignment) ||
    value.assignment.id !== id ||
    value.assignment.company_id !== actor.companyId ||
    (actor.branchId !== null && value.assignment.branch_id !== actor.branchId) ||
    (actor.departmentId !== null && value.assignment.department_id !== actor.departmentId) ||
    !Array.isArray(value.history) ||
    value.history.length > 500 ||
    !value.history.every(event) ||
    (value.handoff !== null && !handoff(value.handoff)) ||
    (value.certificate !== null && !certificate(value.certificate))
  )
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  let previous = 0;
  for (const item of value.history) {
    if (item.assignment_id !== id || item.version <= previous)
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
    previous = item.version;
  }
  if (value.handoff !== null) {
    const item = value.handoff as NativeLearningHandoff;
    if (
      item.assignment_id !== id ||
      item.company_id !== actor.companyId ||
      item.source_assignment_version > value.assignment.version
    )
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  }
  if (value.certificate !== null) {
    const item = value.certificate as NativeLearningCertificate;
    if (
      item.assignment_id !== id ||
      item.company_id !== actor.companyId ||
      item.source_assignment_version > value.assignment.version ||
      value.handoff === null ||
      item.handoff_id !== (value.handoff as NativeLearningHandoff).id
    )
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  }
  return { ...base, ...value } as unknown as NativeLearningDetail;
}

function receipt(
  value: unknown,
  statuses: readonly string[],
  expectedVersion: number,
): NativeLearningReceipt {
  if (
    !exact(value, ['id', 'status', 'version']) ||
    !identifier(value.id) ||
    typeof value.status !== 'string' ||
    !statuses.includes(value.status) ||
    !integer(value.version, 1) ||
    value.version !== expectedVersion
  )
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  return value as unknown as NativeLearningReceipt;
}

function expectedVersion(body: string, field: 'expectedVersion' | 'expectedAssignmentVersion') {
  try {
    const value: unknown = JSON.parse(body);
    if (record(value) && integer(value[field], 1)) return value[field] as number;
  } catch {
    // Fall through to the bounded request failure.
  }
  return failure('NATIVE_LEARNING_REQUEST_INVALID');
}

function context(
  value: unknown,
  actor: NativeLearningPageContext,
  input: NativeLearningReadInput,
): Context {
  if (
    !exact(value, [
      'userId',
      'holdingId',
      'companyId',
      'branchId',
      'departmentId',
      'branchSlug',
      'departmentSlug',
      'assignmentId',
      'assignmentVersion',
      'revision',
      'role',
      'assignmentScopeKind',
      'modules',
      'permissions',
      'expiresAt',
    ])
  )
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (input.branchSlug ?? null) ||
    parsed.departmentSlug !== (input.departmentSlug ?? null) ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !Array.isArray(parsed.modules) ||
    parsed.modules.length > 128 ||
    !parsed.modules.every((item) => identifier(item)) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== (actor.iam?.permissions.length ?? 0) ||
    !parsed.permissions.every((item) => (actor.iam?.permissions ?? []).includes(item))
  )
    return failure('NATIVE_LEARNING_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_LEARNING_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeLearningConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_LEARNING_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30_000 ||
    !Array.isArray(raw.holdings) ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_LEARNING_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_LEARNING_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_LEARNING_CONFIGURATION_INVALID');
    holdings.set(origin.host, {
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeLearningTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  actor: NativeLearningPageContext,
  input: NativeLearningReadInput,
  method: 'GET' | 'POST',
  path: string,
  body: string | undefined,
  key: string | undefined,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== actor.holdingId
  )
    return Promise.reject(new NativeLearningError('NATIVE_LEARNING_AUTHORITY_INVALID'));
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeLearningError('NATIVE_LEARNING_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [header, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      headers.set(header, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  if (method === 'POST') {
    if (!key || !text(key, 180) || key.length < 8 || !/^[A-Za-z0-9:_.-]+$/.test(key))
      return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
    if (body === undefined)
      return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/learning${path}`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
  return transport(method, url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure('NATIVE_LEARNING_RESPONSE_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_LEARNING_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_LEARNING_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_LEARNING_RESPONSE_INVALID');
  }
}

const courseChangeStatuses: Record<string, readonly string[]> = {
  publish: ['published'],
  reject: ['rejected'],
  retire: ['retired'],
};
const assignmentChangeStatuses: Record<string, readonly string[]> = {
  start: ['in_progress'],
  complete: ['completed'],
  waive: ['waived'],
  cancel: ['cancelled'],
};

export function nativeLearningAvailable(): boolean {
  return process.env.ERPX_NATIVE_LEARNING_CONFIG !== undefined;
}

export function nativeLearningConfiguration(raw: string): NativeLearningConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeLearningError('NATIVE_LEARNING_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_LEARNING_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_LEARNING_CONFIGURATION_INVALID');
  return value as unknown as NativeLearningConfiguration;
}

export function nativeLearningFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeLearningError)) return undefined;
  if (error.code === 'NATIVE_LEARNING_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_LEARNING_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return {
      status: 404,
      code: error.code === 'NATIVE_LEARNING_UNAVAILABLE' ? 'NOT_FOUND' : error.code,
      message: error.message,
    };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_LEARNING_UNAVAILABLE',
      message: 'The native Learning service is unavailable.',
    };
  if (error.code === 'NATIVE_LEARNING_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_LEARNING_UNAVAILABLE',
      message: 'The native Learning reader is unavailable.',
    };
  if (error.code === 'NATIVE_LEARNING_REQUEST_INVALID')
    return {
      status: 422,
      code: 'LEARNING_REQUEST_INVALID',
      message: 'Review the Learning request.',
    };
  if (
    error.code === 'NATIVE_LEARNING_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_LEARNING_COOKIE_LIMIT' ||
    error.code === 'NATIVE_LEARNING_RESPONSE_INVALID' ||
    error.code === 'NATIVE_LEARNING_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_LEARNING_UNAVAILABLE',
      message: 'The native Learning reader is unavailable.',
    };
  return undefined;
}

export function createNativeLearningClient(
  raw: NativeLearningConfiguration,
  transport: NativeLearningTransport = directHttp,
) {
  const config = configuration(raw);
  const run = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeLearningPageContext,
    input: NativeLearningReadInput,
    method: 'GET' | 'POST',
    path: string,
    body: string | undefined,
    key: string | undefined,
    signal: AbortSignal | undefined,
    validate: (value: unknown) => unknown,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return failure('NATIVE_LEARNING_CONTEXT_CHANGED');
    const response = await transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      actor,
      input,
      method,
      path,
      body,
      key,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeLearningError) throw error;
      return failure('NATIVE_LEARNING_UNAVAILABLE');
    });
    const value = await boundedJson(response);
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_LEARNING_RESPONSE_INVALID');
    context(value.context, actor, input);
    return validate(value.data);
  };
  return {
    workspace: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) =>
      run(
        incoming,
        actor,
        input,
        'GET',
        `/workspace${input.personal ? '?personal=true' : ''}`,
        undefined,
        undefined,
        signal,
        (value) => workspace(value, actor, input.personal === true),
      ) as Promise<NativeLearningWorkspace>,
    detail: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      id: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'GET',
        `/assignments/${encodeURIComponent(id)}`,
        undefined,
        undefined,
        signal,
        (value) => detail(value, actor, id),
      ) as Promise<NativeLearningDetail>;
    },
    eligibilityFacts: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      personId: string,
      eligibilityCode: string,
      effectiveDate: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) => {
      if (
        !identifier(personId) ||
        personId.includes('/') ||
        personId.includes('\\') ||
        !content(eligibilityCode, 1, 120) ||
        !date(effectiveDate)
      )
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'GET',
        `/eligibility/${encodeURIComponent(personId)}?code=${encodeURIComponent(eligibilityCode)}&effectiveDate=${encodeURIComponent(effectiveDate)}`,
        undefined,
        undefined,
        signal,
        (value) => eligibilityFacts(value, actor, personId, eligibilityCode, effectiveDate),
      ) as Promise<NativeLearningEligibilityFacts>;
    },
    createCourse: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      body: string,
      key: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) =>
      run(incoming, actor, input, 'POST', '/courses', body, key, signal, (value) =>
        receipt(value, ['draft'], 1),
      ) as Promise<NativeLearningReceipt>,
    changeCourse: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      const expected = expectedVersion(body, 'expectedVersion');
      let action: unknown;
      try {
        action = (JSON.parse(body) as Row).action;
      } catch {
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      }
      const statuses =
        typeof action === 'string' ? courseChangeStatuses[action] : undefined;
      if (!statuses)
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/courses/${encodeURIComponent(id)}`,
        body,
        key,
        signal,
        (value) => receipt(value, statuses, expected + 1),
      ) as Promise<NativeLearningReceipt>;
    },
    createAssignment: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      body: string,
      key: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) =>
      run(incoming, actor, input, 'POST', '/assignments', body, key, signal, (value) =>
        receipt(value, ['assigned'], 1),
      ) as Promise<NativeLearningReceipt>,
    changeAssignment: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      const expected = expectedVersion(body, 'expectedVersion');
      let action: unknown;
      try {
        action = (JSON.parse(body) as Row).action;
      } catch {
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      }
      const statuses =
        typeof action === 'string' ? assignmentChangeStatuses[action] : undefined;
      if (!statuses)
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/assignments/${encodeURIComponent(id)}`,
        body,
        key,
        signal,
        (value) => receipt(value, statuses, expected + 1),
      ) as Promise<NativeLearningReceipt>;
    },
    revokeCertificate: (
      incoming: Pick<Headers, 'get'>,
      actor: NativeLearningPageContext,
      id: string,
      body: string,
      key: string,
      input: NativeLearningReadInput,
      signal?: AbortSignal,
    ) => {
      if (!identifier(id) || id.includes('/') || id.includes('\\'))
        return Promise.reject(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID'));
      const expected = expectedVersion(body, 'expectedAssignmentVersion');
      return run(
        incoming,
        actor,
        input,
        'POST',
        `/certificates/${encodeURIComponent(id)}/revoke`,
        body,
        key,
        signal,
        (value) => receipt(value, ['revoked'], expected),
      ) as Promise<NativeLearningReceipt>;
    },
  };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET' | 'POST',
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = request(
      url,
      {
        method,
        agent,
        headers: { ...Object.fromEntries(headers) },
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}
