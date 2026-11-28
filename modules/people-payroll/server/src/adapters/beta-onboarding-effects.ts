import { nativeBetaOnboarding } from "@erpx/collaboration-server/adapters/native-beta-onboarding";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {preparePerson(person:string,id:string,company:string,name:string,email:string,id2:string,now:number,now2:number){return nativeBetaOnboarding(db).person(company,[person, id, company, name, email, id2, now, now2]);},
prepareEmployee(value:string,id:string,company:string,branch:string,department:string,name:string,now:number){return nativeBetaOnboarding(db).employee(company,[value,id,company,branch,department,name,now]);},
prepareEngagement(value:string,person:string,company:string,branch:string,department:string,id:string,now:number,now2:number,value2:string){return nativeBetaOnboarding(db).engagement(company,[value,person,company,branch,department,id,now,now2,value2]);}};}
