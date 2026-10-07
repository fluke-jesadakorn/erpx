import {publicOrigin} from '../domain/public-origin';
export function requestPublicOrigin(request:Request){return publicOrigin({host:request.headers.get('host'),url:request.url},{frontOrigins:[process.env.ERPX_FRONT_PMS_ORIGIN,process.env.ERPX_FRONT_POS_ORIGIN],origin:process.env.ERPX_PUBLIC_ORIGIN,production:process.env.NODE_ENV==='production'});}
