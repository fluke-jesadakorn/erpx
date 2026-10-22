import type {Clock} from "@erpx/server-shared/index";
export interface OrganizationChannelPorts<Result>{clock:Clock;provision(companyId:string,teamAt:number,roomsAt:number):Promise<Result>;}
export function createOrganizationChannelApplication<Result>(ports:OrganizationChannelPorts<Result>){return {ensure(companyId:string){return ports.provision(companyId,ports.clock.now(),ports.clock.now());}};}
