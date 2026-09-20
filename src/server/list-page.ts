import "server-only";
import { createHash } from "node:crypto";
import { ApiError } from "./http";
import type { SqlDatabase } from "./runtime/types";

export type ListPage<T> = { items: T[]; nextCursor: string | null; previousCursor: string | null; hasMore: boolean };
type Cursor = { key: string; id: string; reverse: boolean; scope: string };
/** sql must already enforce read permissions and return unique id and page_key. */
export async function readListPage<T extends { id: string; page_key: string }>(db: SqlDatabase, options: {
  sql: string; args: unknown[]; scope: unknown; cursor?: string; limit?: number; descending?: boolean;
}): Promise<ListPage<T>> {
  const size = Number.isSafeInteger(options.limit) ? Math.max(1, Math.min(options.limit!,100)) : 30;
  const scope = createHash("sha256").update(JSON.stringify([options.scope,!!options.descending,size])).digest("hex");
  let cursor: Cursor | undefined;
  if (options.cursor) {
    try {
      if(options.cursor.length>4096)throw Error();
      const parsed = JSON.parse(Buffer.from(options.cursor, "base64url").toString());
      if (parsed.scope !== scope || typeof parsed.key !== "string" || typeof parsed.id !== "string" || typeof parsed.reverse !== "boolean") throw Error();
      cursor = parsed;
    } catch { throw new ApiError(422, "INVALID_CURSOR", "Restart this search to load results."); }
  }
  const reverse = cursor?.reverse ?? false;
  const descending = Boolean(options.descending) !== reverse;
  const rows = (await db.prepare(`WITH list AS (${options.sql}) SELECT * FROM list
    ${cursor ? `WHERE (page_key,id) ${descending ? "<" : ">"} (?,?)` : ""}
    ORDER BY page_key ${descending ? "DESC" : "ASC"},id ${descending ? "DESC" : "ASC"} LIMIT ?`)
    .bind(...options.args, ...(cursor ? [cursor.key,cursor.id] : []), size + 1).all<T>()).results;
  const overflow = rows.length > size;
  const items = rows.slice(0,size);
  if (reverse) items.reverse();
  const encode = (row: T, backwards: boolean) => Buffer.from(JSON.stringify({key:row.page_key,id:row.id,reverse:backwards,scope})).toString("base64url");
  const previousCursor = items.length && (reverse ? overflow : !!cursor) ? encode(items[0],true) : null;
  const nextCursor = items.length && (reverse ? !!cursor : overflow) ? encode(items[items.length-1],false) : null;
  return { items, previousCursor, nextCursor, hasMore: nextCursor !== null };
}
