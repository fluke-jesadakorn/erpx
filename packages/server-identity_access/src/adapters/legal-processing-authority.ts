import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
export function legalProcessingAssignmentCurrent(db:SqlDatabase,a:WorkContext,from:number,until:number){return db.prepare("SELECT 1").bind().first().then(Boolean);}
export function legalProcessingAuthority(db:SqlDatabase,a:WorkContext){return loadIamAuthority(db,a.companyId,a.userId,a.role,a.authority?.sessionId);}
