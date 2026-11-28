import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned social task-origin binding statement. */
export function socialTaskOriginStatement(
  db: SqlDatabase,
  actor: WorkContext,
  input: {
    key: string;
    post: string;
    task: string;
    version: number;
    hash: string;
    draft: string | null;
    user: string;
    now: number;
  },
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: input.user,
      recordType: 'social_task_origin',
      recordId: input.task,
      recordVersion: input.version,
      message: '',
      kind: 'social_task_origin',
      post_id: input.post,
      source_hash: input.hash,
      ai_draft_id: input.draft ?? undefined,
    },
    input.key,
    input.now,
  );
}
