import type {Clock} from "@erpx/platform-shared-server/index";
import {resolveUsageMonth} from "@erpx/commercial-server/domain/usage";
export function createUsagePeriodApplication(clock:Clock){return(value:string|null)=>resolveUsageMonth(value,clock.now());}
