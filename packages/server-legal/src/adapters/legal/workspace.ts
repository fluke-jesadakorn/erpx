import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {createLegalWorkspaceQuery,type LegalWorkspaceItem} from "@erpx/server-legal/application/workspace";
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
/** Exact projection of the native `workspace` auxiliary read; the transport accepts
 * every authoritative field and no SQL text remains in TypeScript. */
type LegalWorkspaceRead = {deadlines: LegalWorkspaceItem[]; assigned: LegalWorkspaceItem[]};
export async function legalWorkspaceView(_db: SqlDatabase, actor: WorkContext){
  const read=await nativeLegalAuxiliaryRead<LegalWorkspaceRead>(actor,'workspace');
  return createLegalWorkspaceQuery({require:()=>requirePermission(actor,"legal.read"),
    deadlines:async()=>read.deadlines,
    assigned:async()=>read.assigned})();
}
