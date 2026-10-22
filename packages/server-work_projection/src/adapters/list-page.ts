import "server-only";
import {createHash} from "node:crypto";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {ListPage,ListPagePolicy} from "@erpx/server-work_projection/domain/list-page";
import {createListPageReader} from "@erpx/server-work_projection/application/list-page";
/** Caller SQL must enforce authorization and return unique id and page_key. */
export async function readListPage<T extends {id:string;page_key:string}>(db:SqlDatabase,options:ListPagePolicy&{sql:string;args:unknown[]}):Promise<ListPage<T>> {
 return createListPageReader<T>({hash:value=>createHash("sha256").update(value).digest("hex"),decode:cursor=>JSON.parse(Buffer.from(cursor,"base64url").toString()),encode:cursor=>Buffer.from(JSON.stringify(cursor)).toString("base64url"),invalidCursor:()=>new ApiError(422,"INVALID_CURSOR","Restart this search to load results."),rows:async ({cursor,descending,limit})=>(await db.prepare(`WITH list AS (${options.sql}) SELECT * FROM list ${cursor?`WHERE (page_key,id) ${descending?"<":">"} (?,?)`:""} ORDER BY page_key ${descending?"DESC":"ASC"},id ${descending?"DESC":"ASC"} LIMIT ?`).bind(...options.args,...(cursor?[cursor.key,cursor.id]:[]),limit).all<T>()).results})(options);
}
