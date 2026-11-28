import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {createLegalWorkspaceQuery,type LegalWorkspaceItem} from "@erpx/legal-server/application/workspace";
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
