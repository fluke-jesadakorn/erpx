import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../public/authorization";
import {loadIamAuthority} from "../public/iam-store";
export function legalProcessingAssignmentCurrent(db:SqlDatabase,a:WorkContext,from:number,until:number){return db.prepare("SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?").bind(a.assignmentId,a.userId,a.companyId,from,until).first().then(Boolean);}
export function legalProcessingAuthority(db:SqlDatabase,a:WorkContext){return loadIamAuthority(db,a.companyId,a.userId,a.role,a.authority?.sessionId);}
