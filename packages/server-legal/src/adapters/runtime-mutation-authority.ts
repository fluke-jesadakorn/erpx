import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {runtimePreservationFileSources} from '@erpx/server-files/public/runtime-preservation-sources';
import {createRuntimeMutationAuthority} from '@erpx/server-legal/application/runtime-preservation-authority';
/** Read ports for ordinary storage mutations; no destruction command dependency. */
export function runtimeMutationRepository(db:SqlDatabase){const files=runtimePreservationFileSources(db);return {lock:()=>db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run(),pendingOwner:files.pendingOwner,held:async (key:string)=>!!await db.prepare(`SELECT 1 WHERE EXISTS(SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets target ON target.item_id=p.item_id JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state<>'released' AND (p.object_key=? OR (p.source_type='attachment' AND ${files.preservedAttachmentObject}))) OR EXISTS(SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state<>'released' AND target.item_id IN (${files.boundLegalItemIds}))`).bind(key,key,key,key,key).first()};}
export function runtimeMutationAuthority(db:SqlDatabase){return createRuntimeMutationAuthority(runtimeMutationRepository(db));}
