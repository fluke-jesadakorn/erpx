import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {requireAttachmentDescriptor} from './native-lane-36/client';
/** Owner-local Maintenance attachment descriptor. The native axum route is
 * the sole read authority; no descriptor SQL is built or executed here. */
export function policyMaintenanceFacts(_db:SqlDatabase){return {
  readDescriptor:(facts:{targetId:string;companyId:string;targetType:string;}) =>
    facts.targetType === 'maintenance_work'
      ? requireAttachmentDescriptor(facts.targetType,facts.targetId,facts.companyId)
      : Promise.resolve(null),
};}
