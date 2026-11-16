import {runtimeMutationRepository} from "@erpx/server-legal/adapters/runtime-mutation-authority";
import {requireLegalRetentionDeletion} from "@erpx/server-legal/adapters/retention-execution";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {validateLegalDestructionSources,type LegalDestructionManifest} from "@erpx/server-legal/public/legal/destruction-sources";
import {createRuntimePreservationAuthority} from "@erpx/server-legal/application/runtime-preservation-authority";
export function runtimePreservationAuthority(db:SqlDatabase){return createRuntimePreservationAuthority({...runtimeMutationRepository(db),destruction:async id=>{await requireLegalRetentionDeletion(db,id);return db.prepare("SELECT 1").bind().first<{manifest:string|LegalDestructionManifest;item_id:string;company_id:string}>();},validate:(itemId,companyId,manifest)=>validateLegalDestructionSources(db,itemId,companyId,manifest)});}
