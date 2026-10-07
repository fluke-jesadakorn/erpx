import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {requirePermission} from "../../../identity_access/public/iam-permissions";
import {visibility} from "./access";
import {createLegalWorkspaceQuery,type LegalWorkspaceItem} from "../../application/workspace";
export function legalWorkspaceView(db:SqlDatabase,actor:WorkContext){const scope=visibility(actor);return createLegalWorkspaceQuery({require:()=>requirePermission(actor,"legal.read"),
deadlines:async()=>(await db.prepare(`SELECT i.id,i.kind,i.title,i.state,i.due_date,i.owner_id,i.version FROM legal_items i WHERE ${scope.sql} AND i.due_date IS NOT NULL AND i.state NOT IN ('archived','cancelled','closed','completed','terminated') ORDER BY i.due_date,i.id LIMIT 100`).bind(...scope.values).all<LegalWorkspaceItem>()).results,
assigned:async()=>(await db.prepare(`SELECT i.id,i.kind,i.title,i.state,i.due_date,i.owner_id,i.version FROM legal_items i WHERE ${scope.sql} AND i.owner_id=? AND i.state NOT IN ('archived','cancelled','closed','completed','terminated') ORDER BY i.due_date NULLS LAST,i.updated_at DESC LIMIT 100`).bind(...scope.values, actor.userId).all<LegalWorkspaceItem>()).results})()};
