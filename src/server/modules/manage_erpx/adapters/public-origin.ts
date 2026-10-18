import {publicOrigin} from '../domain/public-origin';
import { configuredProductOrigin, productHostForHost } from "@/lib/runtime/product-hosts";
export function requestPublicOrigin(request:Request){
  const productHost = productHostForHost(request.headers.get("host"));
  if (productHost) return configuredProductOrigin(productHost)!;
  return publicOrigin({host:request.headers.get('host'),url:request.url},{frontOrigins:[process.env.ERPX_FRONT_PMS_ORIGIN,process.env.ERPX_FRONT_POS_ORIGIN],origin:process.env.ERPX_PUBLIC_ORIGIN,production:process.env.NODE_ENV==='production'});
}
