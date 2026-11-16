import {runtimeMutationRepository} from "@erpx/server-legal/adapters/runtime-mutation-authority";
import {requireLegalRetentionDeletion} from "@erpx/server-legal/adapters/retention-execution";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {validateLegalDestructionSources,type LegalDestructionManifest} from "@erpx/server-legal/public/legal/destruction-sources";
import {createRuntimePreservationAuthority} from "@erpx/server-legal/application/runtime-preservation-authority";
export function runtimePreservationAuthority(db:SqlDatabase){return createRuntimePreservationAuthority({...runtimeMutationRepository(db),destruction:async id=>{await requireLegalRetentionDeletion(db,id);return db.prepare("SELECT x.manifest,x.item_id,item.company_id FROM legal_destructions x JOIN legal_items item ON item.id=x.item_id WHERE x.id=? AND x.state='executing' AND x.approved_by IS NOT NULL AND NOT EXISTS(SELECT 1 FROM legal_hold_targets target JOIN legal_holds hold ON hold.id=target.hold_id WHERE target.item_id=x.item_id AND hold.state<>'released')").bind(id).first<{manifest:string|LegalDestructionManifest;item_id:string;company_id:string}>();},validate:(itemId,companyId,manifest)=>validateLegalDestructionSources(db,itemId,companyId,manifest)});}
