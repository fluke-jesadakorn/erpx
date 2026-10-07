import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import {ApiError} from "../../../runtime/public/http";
import {createSigningAuthorityQuery} from "../../application/authority";
export function signingAuthority(db:SqlDatabase,itemId:string,userId:string){return createSigningAuthorityQuery({current:async(itemId,userId)=>await db
    .prepare(
      "SELECT a.id,a.version,a.document_id,d.checksum,a.approved_by,a.valid_from,a.valid_until,a.currency,a.limit_minor FROM legal_authorities a JOIN legal_items i ON i.id=a.item_id JOIN legal_contracts c ON c.item_id=i.id JOIN legal_documents d ON d.id=a.document_id WHERE i.id=? AND a.user_id=? AND a.state='approved' AND a.currency=i.currency AND a.limit_minor>=coalesce(nullif(c.data->>'value',''),'0')::numeric AND a.valid_from::date<=(clock_timestamp() AT TIME ZONE i.timezone)::date AND a.valid_until::date>=(clock_timestamp() AT TIME ZONE i.timezone)::date ORDER BY a.valid_until DESC,a.id LIMIT 1",
    )
    .bind(itemId, userId)
    .first<{
      id: string;
      version: number;
      document_id: string;
      checksum: string;
      approved_by: string;
    }>(),error:(status,code,message)=>new ApiError(status,code,message)})(itemId,userId);}
