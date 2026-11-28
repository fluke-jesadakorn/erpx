import {BillingConsole} from '@erpx/billing-ui/components/billing-console';
import {platformBillingPresentation} from '@erpx/billing-server/public/billing-presentation';
export default async function Page({searchParams}:{searchParams:Promise<Record<string,string|string[]|undefined>>}){const query=await searchParams,initial=await platformBillingPresentation(typeof query.holdingId==='string'?query.holdingId:undefined);return <BillingConsole platform writable={initial.writable} initialAccounts={initial.accounts} initialHolding={initial.holdingId} initialData={initial.data}/>;}
