type StripeConnection = {account_id:string|null;livemode:boolean};
type StripeSetupJob = {kind:string;state:string;payload_json:string};

/** Setup links belong to one account and mode, even after the company reconnects. */
export function currentStripeOnboardingUrl(connection:StripeConnection|null,jobs:StripeSetupJob[],now=Date.now()):string|null{
 if(!connection?.account_id)return null;
 for(const job of jobs){
  if(job.state!=='completed'||!['connect','connect_refresh'].includes(job.kind))continue;
  try{
   const payload:unknown=JSON.parse(job.payload_json);
   if(!payload||typeof payload!=='object')continue;
   const value=payload as Record<string,unknown>;
   if(value.accountId!==connection.account_id||value.livemode!==connection.livemode||typeof value.url!=='string'||typeof value.urlExpiresAt!=='number'||value.urlExpiresAt<=now)continue;
   const url=new URL(value.url);
   if(url.protocol==='https:'&&url.hostname==='connect.stripe.com'&&!url.username&&!url.password)return value.url;
  }catch{}
 }
 return null;
}
