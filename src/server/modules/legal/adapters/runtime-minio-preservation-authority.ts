import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {runtimeMinioPreservationSources} from "../../files/public/runtime-minio-preservation-sources";
/** Original remote-store authority; all facts join the supplied preservation transaction. */
export function runtimeMinioPreservationAuthority(db:SqlDatabase){const files=runtimeMinioPreservationSources(db);return {
lock:()=>db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run(),
preservationReady:files.ready,pendingOwner:files.pendingOwner,
held:(key:string)=>db.prepare(`SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE h.state!='released' AND (p.object_key=? OR (p.source_type='attachment' AND ${files.preservedDerivativeObject}))`).bind(key,key).first(),
destruction:(id:string)=>db.prepare("SELECT x.manifest FROM legal_destructions x WHERE x.id=? AND x.state='executing' AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=x.item_id AND h.state!='released')").bind(id).first<{manifest:string}>()
};}
