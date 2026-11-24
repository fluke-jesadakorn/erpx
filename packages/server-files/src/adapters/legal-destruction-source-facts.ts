import { hospitalityOwnerProjectionSources as legalProjectionSources } from "@erpx/server-legal/public/hospitality-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {LINK_SET_JOIN_SQL,PUBLICATION_EXISTS_SQL} from './read-tail-r29-source-sql';
import {ledgerStateRowsSql} from './read-tail-r32-source-sql';
import {STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL} from './read-tail-r41-source-sql';
/** Executes within the caller's preservation transaction and retains exact ledger ordering. */
export function legalDestructionSourceFacts(db:SqlDatabase){return {
canonicalSource:(facts:{id:string;companyId:string;itemId:string;})=>db.prepare("SELECT id,object_key,checksum_sha256,source_state,legal_hold FROM attachments WHERE id=? AND company_id=? AND owner_type='legal_item' AND owner_id=?").bind(facts.id,facts.companyId,facts.itemId).first<{id:string;object_key:string;checksum_sha256:string;source_state:string;legal_hold:number}>(),
storedLedger:(facts:{id:string;})=>db.prepare(ledgerStateRowsSql("state='stored'")).bind(facts.id).all<{id:string;object_key:string;bytes:number;variant:string}>(),
lockedSource:(facts:{id:string;companyId:string;itemId:string;})=>db.prepare("SELECT object_key,checksum_sha256,source_state,legal_hold FROM attachments WHERE id=? AND company_id=? AND owner_type='legal_item' AND owner_id=? FOR UPDATE").bind(facts.id,facts.companyId,facts.itemId).first<{object_key:string;checksum_sha256:string;source_state:string;legal_hold:number}>(),
linkCurrent:(facts:{linkId:string;id:string;companyId:string;itemId:string;})=>db.prepare(`${LINK_SET_JOIN_SQL} owner ON owner.id=link.set_id WHERE link.id=? AND link.attachment_id=? AND owner.company_id=? AND owner.target_type='legal_item' AND owner.target_id=?`).bind(facts.linkId,facts.id,facts.companyId,facts.itemId).first(),
sourceShared:(facts:{id:string;itemId:string;companyId:string;id2:string;itemId2:string;id3:string;itemId3:string;id4:string;})=>db.prepare(`SELECT 1 WHERE EXISTS(${LINK_SET_JOIN_SQL} owner ON owner.id=link.set_id WHERE link.attachment_id=? AND link.state IN ('draft','bound') AND (owner.target_type<>'legal_item' OR owner.target_id<>? OR owner.company_id<>?))
   OR EXISTS(SELECT 1 FROM (${legalProjectionSources.legal_file_preservations}) preservation WHERE preservation.source_type='attachment' AND preservation.source_id=? AND preservation.item_id<>?)
   OR EXISTS(SELECT 1 FROM (${legalProjectionSources.legal_documents}) document WHERE document.attachment_source_id=? AND document.item_id<>?)
   OR ${PUBLICATION_EXISTS_SQL} publication JOIN attachment_links link ON link.id=publication.source_link_id WHERE link.attachment_id=? AND publication.state='approved')`).bind(facts.id,facts.itemId,facts.companyId,facts.id2,facts.itemId2,facts.id3,facts.itemId3,facts.id4).first(),
retainedLedger:(facts:{id:string;})=>db.prepare(ledgerStateRowsSql("state IN ('stored','destroyed')")).bind(facts.id).all<{id:string;object_key:string;bytes:number;variant:string}>(),
objectShared:(facts:{objectKey:string;id:string;})=>db.prepare(`${STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL} AND attachment_id<>? AND state='stored'`).bind(facts.objectKey,facts.id).first()
};}
