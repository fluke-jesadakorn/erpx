import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/server-collaboration/adapters/native-command-assertions";
export const workLayoutClearAssertions=(db:SqlDatabase,key:string,companyId:string)=>nativeClearCommandAssertions(db,companyId,key);
