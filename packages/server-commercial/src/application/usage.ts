import type {Clock} from "@erpx/server-shared/index";
import {resolveUsageMonth} from "@erpx/server-commercial/domain/usage";
export function createUsagePeriodApplication(clock:Clock){return(value:string|null)=>resolveUsageMonth(value,clock.now());}
