import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function runtimeLegalJobHealthEffects(db: SqlDatabase) {
  return {
    renew(at: number, task: string) { return db.prepare('UPDATE legal_runtime_health SET heartbeat_at=? WHERE task=?').bind(at, task).run(); },
    started(task: string, at: number) { return db.prepare(`INSERT INTO legal_runtime_health(task,heartbeat_at) VALUES(?,?)
          ON CONFLICT(task) DO UPDATE SET heartbeat_at=excluded.heartbeat_at`).bind(task, at).run(); },
    succeeded(heartbeatAt: number, successAt: number, task: string) { return db.prepare('UPDATE legal_runtime_health SET heartbeat_at=?,last_success_at=?,failure_count=0,last_error=NULL WHERE task=?').bind(heartbeatAt, successAt, task).run(); },
    failed(at: number, code: string, task: string) { return db.prepare('UPDATE legal_runtime_health SET heartbeat_at=?,failure_count=failure_count+1,last_error=? WHERE task=?').bind(at, code, task).run(); },
  };
}
