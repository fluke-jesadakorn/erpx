import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/collaboration-server/adapters/native-command-assertions";
export function clearAgentBootstrapAssertion(db:SqlDatabase,key:string,companyId:string){return nativeClearCommandAssertions(db,companyId,key);}
