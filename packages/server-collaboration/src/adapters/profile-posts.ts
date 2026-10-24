import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {socialPostScope} from '@erpx/server-collaboration/public/social-source-policy';
export async function memberProfilePostIds(write:SqlDatabase,actor:WorkContext,userId:string,before?:number,beforeId?:string){const postScope=socialPostScope(actor);
    return (await write.prepare(`SELECT p.id FROM social_posts p WHERE p.author_id=? AND ${postScope.sql}
      AND (?::bigint IS NULL OR p.created_at<? OR (p.created_at=? AND (?::text IS NULL OR p.id<?)))
      ORDER BY p.created_at DESC,p.id DESC LIMIT 9`)
      .bind(userId,...postScope.bindings,before??null,before??null,before??null,beforeId??null,beforeId??null).all<{id:string}>()).results;
}
