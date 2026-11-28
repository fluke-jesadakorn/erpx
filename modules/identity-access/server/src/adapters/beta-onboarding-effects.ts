import { nativePrepare } from '@erpx/identity-access-server/public/native-identity-write';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareAssignment(value:string,id:string,value2:"holding_owner" | "employee",company:string,value3:string | null,now:number,value4:number,value5:"holding" | "branch",value6:string | null,value7:string | null,value8:"manual" | "legacy"){return nativePrepare("w_784b5464460c")
      .bind(
        value,
        id,
        value2,
        company,
        value3,
        now,
        value4,
        value5,
        value6,
        value7,
        value8,
      );},
preparePrincipal(value:string,id:string,name:string,now:number){return nativePrepare("w_8ed75e291f41")
      .bind(value, id, name, now);}};}
