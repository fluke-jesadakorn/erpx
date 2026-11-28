import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ppRd40UnlinkedUserByCompany} from "./pp-read-tail-r40-sources";
import {routePeople as nativeRoutePeople} from "./native-lane-9/person-route-client";
export function workReviewPeopleFacts(db:SqlDatabase){return {
 /** Native lane 9 family 3: the Company-scoped statement, projection and
  * response shape are owned by the Rust axum/sqlx service. No TS SQL remains. */
 routePeople:(companyId:string,ids:string[])=>nativeRoutePeople(db,companyId,ids),
 requester:(companyId:string,userId:string)=>db.prepare(ppRd40UnlinkedUserByCompany('legal_name')).bind(companyId,userId).first<{legal_name:string}>()
};}
