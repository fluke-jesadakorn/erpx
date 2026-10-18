import 'server-only';
import {headers} from 'next/headers';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import type {ProjectTaskProductPresentationOptions} from './product-entry';
/** Tighten actual Work-host source presentation; never select an acting role. */
export async function projectTaskProductPresentationOptions():Promise<ProjectTaskProductPresentationOptions>{
 return productHostForHost((await headers()).get('host'))==='work'?{strictProduct:true}:{};
}
