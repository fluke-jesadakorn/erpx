import 'server-only';
import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
export type SqlValueRow=Record<string,string|number|boolean|null>;
export async function one(db: SqlDatabase, sql: string, ...args: unknown[]) { return db.prepare(sql).bind(...args).first<SqlValueRow>(); }
export async function all(db: SqlDatabase, sql: string, ...args: unknown[]) { return (await db.prepare(sql).bind(...args).all<SqlValueRow>()).results; }
export async function run(db: SqlDatabase, sql: string, ...args: unknown[]) { return db.prepare(sql).bind(...args).run(); }
