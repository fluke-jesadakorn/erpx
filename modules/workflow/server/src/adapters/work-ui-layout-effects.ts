import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeClearCommandAssertions } from "@erpx/collaboration-server/adapters/native-command-assertions";
export const workLayoutClearAssertions=(db:SqlDatabase,key:string,companyId:string)=>nativeClearCommandAssertions(db,companyId,key);
