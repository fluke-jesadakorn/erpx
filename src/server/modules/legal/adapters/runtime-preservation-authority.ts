import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {runtimePreservationFileSources} from "../../files/public/runtime-preservation-sources";
import {validateLegalDestructionSources,type LegalDestructionManifest} from "../public/legal/destruction-sources";
import {createRuntimePreservationAuthority} from "../application/runtime-preservation-authority";
export function runtimePreservationAuthority(db:SqlDatabase){const files=runtimePreservationFileSources(db);return createRuntimePreservationAuthority({
 lock:()=>db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run(),pendingOwner:files.pendingOwner,
 held:async key=>!!await db.prepare(`SELECT 1 WHERE EXISTS(SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets target ON target.item_id=p.item_id JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state<>'released' AND (p.object_key=? OR (p.source_type='attachment' AND ${files.preservedAttachmentObject}))) OR EXISTS(SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state<>'released' AND target.item_id IN (${files.boundLegalItemIds}))`).bind(key,key,key,key,key).first(),
 destruction:id=>db.prepare("SELECT x.manifest,x.item_id,item.company_id FROM legal_destructions x JOIN legal_items item ON item.id=x.item_id WHERE x.id=? AND x.state='executing' AND x.approved_by IS NOT NULL AND NOT EXISTS(SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE target.item_id=x.item_id AND hold.state<>'released')").bind(id).first<{manifest:string|LegalDestructionManifest;item_id:string;company_id:string}>(),
 validate:(itemId,companyId,manifest)=>validateLegalDestructionSources(db,itemId,companyId,manifest)
});}
