import type {WorkContext} from "../../identity_access/public/authorization";
export function createStaffPageService<Options,Projects,Staff>(ports:{actor(slug:string):Promise<WorkContext>;prepare(actor:WorkContext):Promise<void>;settings(actor:WorkContext):Promise<Options>;projects(actor:WorkContext):Promise<Projects>;staff(actor:WorkContext):Promise<Staff>;expectedAbsence(error:unknown):boolean;base(actor:WorkContext):string}) {
 return async function staffPage(slug:string){const actor=await ports.actor(slug);await ports.prepare(actor);const [options,projects,staff]=await Promise.all([ports.settings(actor),ports.projects(actor),ports.staff(actor).catch(error=>{if(ports.expectedAbsence(error))return null;throw error;})]);return {actor,base:ports.base(actor),options,projects,staff};};
}
