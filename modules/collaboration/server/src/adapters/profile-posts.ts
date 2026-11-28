import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {socialPostScope} from '@erpx/collaboration-server/public/social-source-policy';
import {socialPostIdHead} from './read-tail-sql';
export async function memberProfilePostIds(write:SqlDatabase,actor:WorkContext,userId:string,before?:number,beforeId?:string){const postScope=socialPostScope(actor);
    return (await write.prepare(`${socialPostIdHead(`p.author_id=? AND ${postScope.sql}
      AND (?::bigint IS NULL OR p.created_at<? OR (p.created_at=? AND (?::text IS NULL OR p.id<?)))
      ORDER BY p.created_at DESC,p.id DESC LIMIT 9`)}`)
      .bind(userId,...postScope.bindings,before??null,before??null,before??null,beforeId??null,beforeId??null).all<{id:string}>()).results;
}
