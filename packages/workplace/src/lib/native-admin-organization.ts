import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import {
  nativeOrganizationAccessPage,
  nativeOrganizationFailure,
  nativeOrganizationWorkspace,
} from '@erpx/feature-organization/lib/native-organization';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { OrganizationNode, OrganizationPosition } from '@erpx/feature-organization/lib/organization';
import type { OrganizationAccessData } from '@erpx/feature-organization/lib/organization-access';

type Row = Record<string, unknown>;

/** The bounded shape this Admin projection consumes from the existing
 * Organization owner port. The owner client validates the complete response;
 * this type keeps the Admin read independent of its adapter module. */
export type AdminNativeOrganizationWorkspace = Readonly<{
  nodes: readonly unknown[];
  positions: readonly unknown[];
  levels: readonly Readonly<{ id: string; name: string; level_number: number }>[];
  actorId: string;
  canManage: boolean;
}>;

/** Admin owns this projection; the Organization owner remains the only source
 * of nodes, positions and levels. A native response is accepted only when it
 * still names the current Company and actor, then the retained People rows and
 * Company-manage ceiling are preserved. */
export type AdminOrganizationActor = Readonly<{
  userId: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
}>;

const invalid = (message = 'The native organization workspace is not valid for this Company.'): never => {
  throw new ApiError(502, 'NATIVE_ORGANIZATION_RESPONSE_INVALID', message);
};

const row = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= maximum;

const nullableText = (value: unknown, maximum = 4096): value is string | null =>
  value === null || text(value, maximum);

const integer = (value: unknown, minimum = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;

type AdminNode = OrganizationNode & { company_id: string };

function node(value: unknown): AdminNode {
  if (
    !row(value) ||
    !text(value.id, 200) ||
    !text(value.kind, 40) ||
    !text(value.slug, 80) ||
    !text(value.display_name, 120) ||
    !nullableText(value.parent_id, 200) ||
    !text(value.company_id, 200) ||
    !nullableText(value.branch_id, 200) ||
    !['active', 'suspended', 'archived'].includes(String(value.status)) ||
    !integer(value.hierarchy_revision, 1)
  )
    return invalid();
  return {
    id: value.id,
    kind: value.kind,
    slug: value.slug,
    display_name: value.display_name,
    branch_id: value.branch_id,
    status: value.status as string,
    hierarchy_revision: value.hierarchy_revision,
    company_id: value.company_id,
  };
}

function position(value: unknown): OrganizationPosition {
  if (
    !row(value) ||
    !text(value.id, 200) ||
    !text(value.company_id, 200) ||
    !nullableText(value.branch_id, 200) ||
    !nullableText(value.department_id, 200) ||
    !text(value.title, 200) ||
    !text(value.level_id, 200) ||
    !integer(value.level_number, 1) ||
    !nullableText(value.reports_to_position_id, 200) ||
    typeof value.requirements !== 'string' ||
    !integer(value.is_ceo) ||
    !integer(value.roster_required) ||
    !integer(value.active) ||
    !integer(value.version, 1)
  )
    return invalid();
  return {
    id: value.id,
    company_id: value.company_id,
    branch_id: value.branch_id,
    department_id: value.department_id,
    title: value.title,
    level_id: value.level_id,
    level_number: value.level_number,
    reports_to_position_id: value.reports_to_position_id,
    requirements: value.requirements,
    is_ceo: value.is_ceo,
    roster_required: value.roster_required,
    active: value.active,
    version: value.version,
  };
}

type AdminLevel = { id: string; name: string; level_number: number };

function level(value: unknown): AdminLevel {
  if (
    !row(value) ||
    !text(value.id, 200) ||
    !text(value.name, 120) ||
    !integer(value.level_number, 1)
  )
    return invalid();
  return { id: value.id, name: value.name, level_number: value.level_number };
}

function unique(ids: readonly string[]): boolean {
  return new Set(ids).size === ids.length;
}

/** Compose only the Organization-owned tree into the retained Admin page read.
 * The retained People rows, private-field redaction and `canMovePeople` remain
 * authoritative; native data never raises the existing Company-manage ceiling. */
export function composeAdminOrganizationAccess(
  retained: OrganizationAccessData,
  native: AdminNativeOrganizationWorkspace,
  actor: AdminOrganizationActor,
): OrganizationAccessData {
  if (native.actorId !== actor.userId)
    return invalid('The native organization workspace belongs to another actor.');
  const nodes = native.nodes.map(node);
  const positions = native.positions.map(position);
  const levels = native.levels.map(level);
  if (
    nodes.some((item) => item.company_id !== actor.companyId) ||
    positions.some((item) => item.company_id !== actor.companyId) ||
    !unique(nodes.map((item) => item.id)) ||
    !unique(positions.map((item) => item.id)) ||
    !unique(levels.map((item) => item.id))
  )
    return invalid('The native organization workspace is not scoped to this Company.');
  const inScope = (item: OrganizationPosition) =>
    item.active === 1 &&
    (actor.branchId === null || item.branch_id === actor.branchId) &&
    (actor.departmentId === null || item.department_id === actor.departmentId);
  return {
    positions: positions.filter(inScope),
    people: retained.people,
    nodes,
    levels,
    canManage: retained.canManage && native.canManage,
    canMovePeople: retained.canMovePeople,
    actorId: native.actorId,
  };
}

/** Explicit opt-in through ERPX_NATIVE_ORGANIZATION_CONFIG. A configured
 * failure is mapped to the existing bounded API error and never silently
 * falls back to the retained Organization reader. Actors without
 * `people.manage` keep the retained page because the current Organization
 * native HTTP read is gated by that permission. */
export async function nativeAdminOrganization(
  actor: WorkContext,
  holdingSlug: string,
  query: Record<string, string | undefined>,
  incoming: Pick<Headers, 'get'>,
): Promise<AdminNativeOrganizationWorkspace | undefined> {
  if (actor.holdingSlug !== holdingSlug)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload before reading the Company.');
  if (!hasPermission(actor, 'people.manage')) return undefined;
  try {
    return await nativeOrganizationWorkspace(actor, holdingSlug, query, incoming);
  } catch (error) {
    const failure = nativeOrganizationFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** One Admin read boundary for the SSR page and its visitor-facing JSON API.
 * The retained projection is still read first so People/current-private
 * redaction remain authoritative; replacing it is all-or-nothing. */
export async function readAdminOrganization(
  retained:
    | OrganizationAccessData
    | Promise<OrganizationAccessData>
    | (() => Promise<OrganizationAccessData>),
  actor: WorkContext,
  holdingSlug: string,
  query: Record<string, string | undefined>,
  incoming: Pick<Headers, 'get'>,
): Promise<OrganizationAccessData> {
  void query;
  const projection = () =>
    typeof retained === 'function' ? retained() : Promise.resolve(retained);
  return nativeOrganizationAccessPage(actor, holdingSlug, incoming, projection);
}
