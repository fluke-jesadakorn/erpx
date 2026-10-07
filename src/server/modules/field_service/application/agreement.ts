import {createAgreementSchema,type AgreementInput} from "../domain/agreement";
import {BusinessRuleError,type Clock} from "../../shared";
export interface AgreementPorts {clock:Clock;branchId:string|null;departmentId:string|null;authorize():Promise<void>;replay(key:string,operation:string,id:string,input:AgreementInput):Promise<Record<string,unknown>|null>;persist(command:{key:string;operation:string;id:string;input:AgreementInput;branch:string;department:string|null;now:number}):Promise<Record<string,unknown>>;}
export function createAgreementApplication(ports:AgreementPorts){return async(raw:unknown,key:string)=>{
 await ports.authorize();const input=createAgreementSchema.parse(raw),branch=ports.branchId??input.branchId,department=ports.departmentId??input.departmentId??null;
 if(input.endDate<input.startDate)throw new BusinessRuleError(422,"FIELD_SERVICE_DATE_RANGE","Agreement end date must be on or after its start date.");
 const id=`service-agreement-${key}`,operation="field.agreement.create",prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 return ports.persist({key,operation,id,input,branch,department,now:ports.clock.now()});
};}
