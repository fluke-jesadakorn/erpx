import {publicOrigin} from '@erpx/manage-erpx-server/domain/public-origin';
import { requestProductOrigin, productHostForHost } from "@erpx/platform-runtime-ui/lib/product-hosts";
export function requestPublicOrigin(request:Request){
  const productHost = productHostForHost(request.headers.get("host"));
  if (productHost) return requestProductOrigin(productHost,request.headers)!;
  return publicOrigin({host:request.headers.get('host'),url:request.url},{frontOrigins:[process.env.ERPX_FRONT_PMS_ORIGIN,process.env.ERPX_FRONT_POS_ORIGIN],origin:process.env.ERPX_PUBLIC_ORIGIN,production:process.env.NODE_ENV==='production'});
}
