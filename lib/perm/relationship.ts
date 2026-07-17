import 'server-only';
import { query } from '@folio-lib/db';

export type RelKind = 'owner' | 'approver' | 'watcher' | 'submitter';

export interface RelTuple {
  src_user_id: number;
  kind: RelKind;
  object_key: string;
}

export async function getRelationshipsForUser(
  userId: number,
  kinds?: RelKind[],
): Promise<RelTuple[]> {
  const kindFilter = kinds && kinds.length > 0 ? kinds : null;
  const { rows } = await query<RelTuple>(
    `SELECT src_user_id, kind, object_key FROM perm.relationships
      WHERE src_user_id = $1
        ${kindFilter ? `AND kind = ANY($2)` : ``}`,
    kindFilter ? [userId, kindFilter] : [userId],
  );
  return rows;
}

export async function userOwnsObject(userId: number, objectKey: string): Promise<boolean> {
  const r = await query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM perm.relationships
        WHERE src_user_id = $1 AND kind = 'owner' AND object_key = $2
     ) AS exists`,
    [userId, objectKey],
  );
  return Boolean(r.rows[0]?.exists);
}

export async function check(
  userId: number,
  kind: RelKind,
  objectKey: string,
): Promise<boolean> {
  const r = await query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM perm.relationships
        WHERE src_user_id = $1 AND kind = $2 AND object_key = $3
     ) AS exists`,
    [userId, kind, objectKey],
  );
  return Boolean(r.rows[0]?.exists);
}
