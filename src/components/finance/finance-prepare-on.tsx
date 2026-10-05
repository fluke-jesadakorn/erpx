"use client";
import {Field,TextInput} from "@/components/ui/foundation";
export function FinancePrepareOn(){return <Field appearance="standard">Prepare draft on (optional)<TextInput appearance="standard" type="date" name="prepareOn"/><small className="muted">Leave empty to prepare now. Scheduled work retains the source version and waits for independent review.</small></Field>;}
export function financeDraftCommand(path:string,kind:"billing"|"recognition"|"asset_depreciation"|"instrument_payment",sourceId:unknown,form:FormData,input:Record<string,unknown>):[string,Record<string,unknown>]{const runOn=form.get("prepareOn");return runOn?["due-drafts",{kind,sourceId:String(sourceId),runOn,input,reason:input.reason??"Prepare this billing draft on its chosen date"}]:[path,input];}
