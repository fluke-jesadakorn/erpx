import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {FlowKind} from "@erpx/platform-shared-ui/lib/flow";
export interface FlowAccessPorts{prepare(holdingId:string):Promise<void>;ownerRead(actor:WorkContext,kind:FlowKind,id:string):Promise<void>;}
export function createFlowAccessApplication(ports:FlowAccessPorts){return {async requireFlowAccess(actor:WorkContext,kind:FlowKind,id:string){await ports.prepare(actor.holdingId);await ports.ownerRead(actor,kind,id);}};}
