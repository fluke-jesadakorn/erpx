import 'server-only';
import { canonicalDestinationLinks } from '@erpx/feature-work_projection/lib/destination-links';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { attachmentsV2ReadOnly } from '@erpx/server-files/public/attachments/feature';
import {
  createNativeAttachmentReadClient,
  nativeAttachmentReadAvailable,
  nativeAttachmentReadConfiguration,
  nativeAttachmentReadFailure,
  type NativeAttachmentListInput,
  type NativeAttachmentPageContext,
} from '@erpx/server-files/public/native-attachments';

function pageContext(actor: WorkContext): NativeAttachmentPageContext {
  if (!actor.authority || !actor.iam)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload before reading files.');
  const scope =
    actor.scope.kind === 'holding'
      ? ({ kind: 'holding' } as const)
      : actor.scope.kind === 'branch'
        ? ({ kind: 'branch', branchSlug: actor.scope.branchSlug } as const)
        : ({ kind: 'department', branchSlug: actor.scope.branchSlug, departmentSlug: actor.scope.departmentSlug } as const);
  return {
    userId: actor.userId,
    holdingId: String(actor.holdingId),
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
    modules: actor.modules,
    iam: { permissions: actor.iam.permissions },
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeAttachmentReadClient(nativeAttachmentReadConfiguration(raw));
}

function failed(error: unknown): never {
  const mapped = nativeAttachmentReadFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  if (error instanceof ApiError) throw error;
  throw new ApiError(503, 'NATIVE_ATTACHMENT_READER_UNAVAILABLE', 'The native file reader is unavailable.');
}

export function nativeAttachmentReadConfigured(): boolean {
  return nativeAttachmentReadAvailable();
}

/** Returns `undefined` only when no native configuration exists. A configured
 * native failure is surfaced and never replaced by the retained TypeScript read. */
export async function nativeAttachmentSets(
  request: Request,
  actor: WorkContext,
  search: URLSearchParams,
): Promise<Response | undefined> {
  const reader = client();
  if (!reader) return undefined;
  try {
    const componentKind = search.get('componentKind') ?? undefined;
    const expectedTargetVersion = search.get('expectedTargetVersion');
    const input: NativeAttachmentListInput = {
      holdingSlug: actor.holdingSlug,
      targetType: search.get('targetType') ?? '',
      targetId: search.get('targetId') ?? '',
      ...(search.get('purposeKey') ? { purposeKey: search.get('purposeKey')! } : {}),
      ...(componentKind ? { componentKind } : {}),
      ...(expectedTargetVersion
        ? { expectedTargetVersion: Number(expectedTargetVersion) }
        : {}),
      readOnly: attachmentsV2ReadOnly(),
    };
    const sets = await reader.list(request.headers, pageContext(actor), input);
    return Response.json(canonicalDestinationLinks({ data: sets }), {
      headers: { 'Cache-Control': 'private, no-store' },
    });
  } catch (error) {
    failed(error);
  }
}

export async function nativeAttachmentStatus(
  request: Request,
  actor: WorkContext,
  linkId: string,
): Promise<Response | undefined> {
  const reader = client();
  if (!reader) return undefined;
  try {
    const status = await reader.status(
      request.headers,
      pageContext(actor),
      actor.holdingSlug,
      linkId,
    );
    return Response.json(canonicalDestinationLinks({ data: status }), {
      headers: { 'Cache-Control': 'private, no-store' },
    });
  } catch (error) {
    failed(error);
  }
}
