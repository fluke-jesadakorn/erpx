import {runtimeMinioMutationAuthority} from "@erpx/legal-server/adapters/runtime-minio-mutation-authority";
import {requireLegalRetentionDeletion} from "@erpx/legal-server/adapters/retention-execution";
import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {legalHoldGuardPrefix,legalHoldGuardSuffix} from './legal-hold-guard-sources';
/** Original remote-store authority; all facts join the supplied preservation transaction. */
export function runtimeMinioPreservationAuthority(db:SqlDatabase){return {...runtimeMinioMutationAuthority(db),destruction:async(id:string)=>{await requireLegalRetentionDeletion(db,id);return db.prepare("SELECT x.manifest FROM legal_destructions x WHERE x.id=? AND x.state='executing' AND "+legalHoldGuardPrefix+"x.item_id"+legalHoldGuardSuffix).bind(id).first<{manifest:string}>();}};}
