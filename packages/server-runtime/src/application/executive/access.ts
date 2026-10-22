import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';
export interface ExecutiveAccessRepository{authority(actor:WorkContext,now:number):Promise<boolean>;eligibleEngagement(actor:WorkContext):Promise<boolean>;sharedReport(actor:WorkContext,now:number):Promise<boolean>;}
export interface ExecutiveAccessPorts{repository:ExecutiveAccessRepository;clock:Clock;}
export function createExecutiveAccessApplication(ports:ExecutiveAccessPorts){
 async function currentAuthority(actor:WorkContext){if(!await ports.repository.authority(actor,ports.clock.now()))throw new BusinessRuleError(403,'EXECUTIVE_ACCESS_CHANGED','Your access changed. Reload and select an active work role.');}
 async function executiveEligible(actor:WorkContext){if(actor.role==='holding_owner'&&hasPermission(actor,'company.owner'))return true;return ports.repository.eligibleEngagement(actor);}
 async function canAuthor(actor:WorkContext){return hasPermission(actor,'executive.access')&&hasPermission(actor,'executive.report.create')&&await executiveEligible(actor);}
 async function requireAuthor(actor:WorkContext,permission='executive.report.create'){await currentAuthority(actor);if(!await canAuthor(actor)||!hasPermission(actor,permission))throw new BusinessRuleError(403,'EXECUTIVE_AUTHOR_REQUIRED','Executive authoring requires an active Level 1 or Level 2 engagement and a reviewed Executive access grant. Holding owners are also eligible.');}
 async function executiveVisible(actor:WorkContext){return await canAuthor(actor)||await ports.repository.sharedReport(actor,ports.clock.now());}
 return {currentAuthority,executiveEligible,canAuthor,requireAuthor,executiveVisible};
}
