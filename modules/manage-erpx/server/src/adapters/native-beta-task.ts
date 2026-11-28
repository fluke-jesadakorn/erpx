import 'server-only';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeStaffWorkCommand } from '@erpx/operations-server/public/native-staff-work-page';

type TaskInput = {
  title: string;
  description: string;
  assigneeUserId: string;
  projectId: string | null;
  startApprovers: string[];
  completionApprovers: string[];
  dueAt: string | null;
  expectedVersion: number;
  reason: string;
};

type TaskSource = {
  postId: string;
  sourceVersion: number;
  sourceHash: string;
  commentIds: string[];
  aiDraftId: string | null;
};

function configuration() {
  const raw = process.env.ERPX_NATIVE_OPERATIONS_CONFIG;
  if (!raw) throw new Error('NATIVE_OPERATIONS_UNAVAILABLE');
  const value = JSON.parse(raw) as {
    holdings: { origin: string; holdingSlug: string; holdingId: string }[];
  };
  return value;
}

/** Create a task from a reviewed social post through the native Staff Work owner. */
export async function saveTaskNative(
  actor: WorkContext,
  input: TaskInput,
  key: string,
  source: TaskSource,
): Promise<unknown> {
  const config = configuration();
  const target = config.holdings.find((holding) => holding.holdingSlug === actor.holdingSlug);
  if (!target || target.holdingId !== actor.holdingId) throw new Error('NATIVE_OPERATIONS_UNAVAILABLE');
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  });
  if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  const request = new Request(`${target.origin}/api/v1/holdings/${actor.holdingSlug}`, {
    method: 'POST',
    headers,
  });
  const body = {
    ...input,
    source: {
      postId: source.postId,
      sourceVersion: source.sourceVersion,
      sourceHash: source.sourceHash,
      commentIds: source.commentIds,
      aiDraftId: source.aiDraftId,
    },
  };
  return nativeStaffWorkCommand(request, actor.holdingSlug, '', body, key);
}
