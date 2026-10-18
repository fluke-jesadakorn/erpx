import type {LegalDetail} from '@/server/modules/legal/public/rules/model';
/** The browser uses owner download routes. Canonical storage and private File
 * source/link identities stay with server readers and original owner policy. */
export function legalWorkspaceSnapshot(detail:LegalDetail):LegalDetail{return {...detail,documents:detail.documents.map(document=>({id:document.id,item_id:document.item_id,version:document.version,name:document.name,media_type:document.media_type,checksum:document.checksum,bytes:document.bytes,body:document.body,created_by:document.created_by,created_at:document.created_at,locked:document.locked,object_key:null}))};}
