import type {Clock} from "../../shared";
import {resolveUsageMonth} from "../domain/usage";
export function createUsagePeriodApplication(clock:Clock){return(value:string|null)=>resolveUsageMonth(value,clock.now());}
