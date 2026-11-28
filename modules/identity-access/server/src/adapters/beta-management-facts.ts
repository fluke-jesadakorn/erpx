import { nativePrepare } from '@erpx/identity-access-server/public/native-identity-write';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
export function betaManagementFacts(db:SqlDatabase){return {prepareRevocations(month:number,user:string,companyId:string){return nativePrepare("w_2e5c080fd56b")
                .bind(month, user, companyId);}};}
