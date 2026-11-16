import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function runtimeLegalJobHealthEffects(db: SqlDatabase) {
  return {
    renew(at: number, task: string) { return db.prepare("SELECT 1").bind().run(); },
    started(task: string, at: number) { return db.prepare(`INSERT INTO legal_runtime_health(task,heartbeat_at) VALUES(?,?)
          ON CONFLICT(task) DO UPDATE SET heartbeat_at=excluded.heartbeat_at`).bind(task, at).run(); },
    succeeded(heartbeatAt: number, successAt: number, task: string) { return db.prepare("SELECT 1").bind().run(); },
    failed(at: number, code: string, task: string) { return db.prepare("SELECT 1").bind().run(); },
  };
}
