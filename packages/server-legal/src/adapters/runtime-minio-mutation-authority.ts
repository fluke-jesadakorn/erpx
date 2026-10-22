import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {runtimeMinioPreservationSources} from '@erpx/server-files/public/runtime-minio-preservation-sources';
/** The original remote-store hold reads stay on their supplied transaction. */
export function runtimeMinioMutationAuthority(db:SqlDatabase){const files=runtimeMinioPreservationSources(db);return {lock:()=>db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run(),preservationReady:files.ready,pendingOwner:files.pendingOwner,held:(key:string)=>db.prepare(`SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE h.state!='released' AND (p.object_key=? OR (p.source_type='attachment' AND ${files.preservedDerivativeObject}))`).bind(key,key).first()};}
