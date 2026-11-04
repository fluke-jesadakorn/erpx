import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { PmsWorkspace } from '@erpx/server-pms/public/pms';
import {
  createNativePmsClient,
  nativePmsConfiguration,
  NativePmsError,
  type NativePmsBoardSection,
  type NativePmsPageContext,
  type NativePmsReadInput,
} from '@erpx/feature-pms/adapters/server/native-pms';

function expected(actor: NativePmsPageContext) {
  if (!actor.authority) throw new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: actor.authority.revision,
    assignmentVersion: actor.authority.assignmentVersion,
  } as const;
}

function pageContext(actor: WorkContext): NativePmsPageContext {
  if (!actor.authority || !actor.iam) throw new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED');
  const scope =
    actor.scope.kind === 'holding'
      ? ({ kind: 'holding' } as const)
      : actor.scope.kind === 'branch'
        ? ({ kind: 'branch', branchSlug: actor.scope.branchSlug } as const)
        : ({
            kind: 'department',
            ...(actor.scope.branchSlug ? { branchSlug: actor.scope.branchSlug } : {}),
            departmentSlug: actor.scope.departmentSlug,
          } as const);
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    scope,
    authority: {
      revision: actor.authority.revision,
      assignmentVersion: actor.authority.assignmentVersion,
    },
    modules: [...actor.modules],
    iam: { permissions: [...actor.iam.permissions] },
  };
}

function scopeInput(actor: NativePmsPageContext) {
  if (actor.scope.kind === 'holding') return {};
  if (actor.scope.kind === 'branch') return { branchSlug: actor.scope.branchSlug };
  return {
    ...(actor.scope.branchSlug ? { branchSlug: actor.scope.branchSlug } : {}),
    departmentSlug: actor.scope.departmentSlug,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_PMS_CONFIG;
  if (raw === undefined) return undefined;
  return createNativePmsClient(nativePmsConfiguration(raw));
}

export function nativePmsAvailable(): boolean {
  return process.env.ERPX_NATIVE_PMS_CONFIG !== undefined;
}

export type NativePmsBoardSearch = Readonly<{
  property?: string;
  status?: string;
  q?: string;
  building?: string;
  floor?: string;
  typeId?: string;
  page?: string;
  selectedId?: string;
  record?: string;
  detailId?: string;
  from?: string;
  to?: string;
}>;

function pageNumber(value: string | undefined): number | undefined {
  if (value === undefined || value === '') return undefined;
  if (!/^\d{1,5}$/.test(value)) throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
  const page = Number(value);
  if (!Number.isSafeInteger(page) || page > 10_000)
    throw new NativePmsError('NATIVE_PMS_REQUEST_INVALID');
  return page;
}

export async function nativePmsRoomBoard(
  actor: WorkContext,
  holdingSlug: string,
  section: NativePmsBoardSection,
  search: NativePmsBoardSearch = {},
): Promise<PmsWorkspace | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED');
  const page = pageContext(actor);
  const pageIndex = pageNumber(search.page);
  const input: NativePmsReadInput = {
    holdingSlug,
    section,
    ...(search.property ? { property: search.property } : {}),
    ...(search.status ? { status: search.status } : {}),
    ...(search.q ? { q: search.q } : {}),
    ...(search.building ? { building: search.building } : {}),
    ...(search.floor ? { floor: search.floor } : {}),
    ...(search.typeId ? { typeId: search.typeId } : {}),
    ...(pageIndex !== undefined ? { page: pageIndex } : {}),
    ...(search.selectedId ?? search.record ?? search.detailId
      ? { selectedId: search.selectedId ?? search.record ?? search.detailId }
      : {}),
    ...(search.from ? { from: search.from } : {}),
    ...(search.to ? { to: search.to } : {}),
    ...scopeInput(page),
    expected: expected(page),
  };
  return native.roomBoard(await headers(), page, input);
}

export async function nativePmsRoomBoardFromSearch(
  actor: WorkContext,
  holdingSlug: string,
  section: NativePmsBoardSection,
  search: Readonly<Record<string, string | string[] | undefined>>,
): Promise<PmsWorkspace | undefined> {
  const value = (key: string) => (typeof search[key] === 'string' ? search[key] : undefined);
  return nativePmsRoomBoard(actor, holdingSlug, section, {
    property: value('property'),
    status: value('status'),
    q: value('q'),
    building: value('building'),
    floor: value('floor'),
    typeId: value('typeId'),
    page: value('page'),
    selectedId: value('selectedId'),
    record: value('record'),
    detailId: value('detailId'),
    from: value('from'),
    to: value('to'),
  });
}

export { NativePmsError, nativePmsFailure } from '@erpx/feature-pms/adapters/server/native-pms';
