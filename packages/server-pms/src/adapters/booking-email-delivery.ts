import 'server-only';
import {holdingRoutingEnabled,holdingPublicUiUrl} from '@erpx/contracts/holding-hosts';
import {applicationOrigin} from '@erpx/feature-runtime/lib/application-origins';
import {configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import type {BookingEnv} from '@erpx/server-pms/domain/booking';

/** Deliver an already queued private link on its current public host. The
 * encrypted source, token, expiry and original booking remain unchanged. */
export function bookingEmailDeliveryPayload(value:string,env:BookingEnv):string{
 if(holdingRoutingEnabled(process.env)&&env.origin==='https://console-dev-erpx.jesadakorn.com'){
  const payload=JSON.parse(value) as {text?:unknown};
  if(typeof payload?.text!=='string')return value;
  const text=payload.text.replace(/https:\/\/(?:console|booking)-dev-erpx\.jesadakorn\.com\/book\/[^\s]+/g,source=>holdingPublicUiUrl(source)??source);
  return text===payload.text?value:JSON.stringify({...payload,text});
 }
 const booking=configuredProductOrigin('booking');
 if(!booking||applicationOrigin('booking',env.origin)!==booking)return value;
 const payload:unknown=JSON.parse(value);
 if(!payload||typeof payload!=='object'||Array.isArray(payload)||!('text' in payload)||typeof payload.text!=='string')return value;
 const text=payload.text.replace(/https:\/\/console-dev-erpx\.jesadakorn\.com\/book\/[^\s]+/g,original=>{
  const url=new URL(original);
  if(url.origin!=='https://console-dev-erpx.jesadakorn.com'||!/^\/book\/[^/]+\/manage$/.test(url.pathname)||!url.hash.startsWith('#token='))return original;
  return new URL(url.pathname+url.search+url.hash,booking).href;
 });
 return text===payload.text?value:JSON.stringify({...payload,text});
}
