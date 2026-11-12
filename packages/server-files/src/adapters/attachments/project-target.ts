import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/** Resolve the owning project id for a Files set or link, or null when the
 * target is not a project. Files-owned SQL; callers keep their scope checks. */
export async function projectAttachmentTargetId(
  db: SqlDatabase,
  companyId: string,
  kind: 'set' | 'link',
  id: string,
): Promise<string | null> {
  const row =
    kind === 'set'
      ? await db
          .prepare(
            "SELECT target_id FROM attachment_sets WHERE id=? AND company_id=? AND target_type='project'",
          )
          .bind(id, companyId)
          .first<{ target_id: string }>()
      : await db
          .prepare(
            "SELECT s.target_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=? WHERE l.id=? AND s.target_type='project'",
          )
          .bind(companyId, id)
          .first<{ target_id: string }>();
  return row?.target_id ?? null;
}
