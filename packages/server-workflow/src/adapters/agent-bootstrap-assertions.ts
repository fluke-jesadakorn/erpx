import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/server-collaboration/adapters/native-command-assertions";
export function clearAgentBootstrapAssertion(db:SqlDatabase,key:string,companyId:string){return nativeClearCommandAssertions(db,companyId,key);}
