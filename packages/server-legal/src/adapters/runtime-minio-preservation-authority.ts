import {runtimeMinioMutationAuthority} from "@erpx/server-legal/adapters/runtime-minio-mutation-authority";
import {requireLegalRetentionDeletion} from "@erpx/server-legal/adapters/retention-execution";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Original remote-store authority; all facts join the supplied preservation transaction. */
export function runtimeMinioPreservationAuthority(db:SqlDatabase){return {...runtimeMinioMutationAuthority(db),destruction:async(id:string)=>{await requireLegalRetentionDeletion(db,id);return db.prepare("SELECT x.manifest FROM legal_destructions x WHERE x.id=? AND x.state='executing' AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=x.item_id AND h.state!='released')").bind(id).first<{manifest:string}>();}};}
