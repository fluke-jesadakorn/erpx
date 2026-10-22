import type {RecordScope} from '@erpx/server-operations/public/staff-policy';import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function overtimeNotification(
  db: SqlDatabase,
  scope: RecordScope,
  userId: string,
  type: string,
  id: string,
  version: number,
  message: string,
) {
  return db
    .prepare(
      "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",
    )
    .bind(
      crypto.randomUUID(),
      scope.company_id,
      userId,
      type,
      id,
      version,
      message,
      Date.now(),
    );
}
