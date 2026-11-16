import {runtimeMinioMutationAuthority} from "@erpx/server-legal/adapters/runtime-minio-mutation-authority";
import {requireLegalRetentionDeletion} from "@erpx/server-legal/adapters/retention-execution";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Original remote-store authority; all facts join the supplied preservation transaction. */
export function runtimeMinioPreservationAuthority(db:SqlDatabase){return {...runtimeMinioMutationAuthority(db),destruction:async(id:string)=>{await requireLegalRetentionDeletion(db,id);return db.prepare("SELECT 1").bind().first<{manifest:string}>();}};}
