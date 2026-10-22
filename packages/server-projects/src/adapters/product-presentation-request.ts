import 'server-only';
import {headers} from 'next/headers';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import type {ProjectTaskProductPresentationOptions} from '@erpx/server-projects/adapters/product-entry';
/** Tighten actual Work-host source presentation; never select an acting role. */
export async function projectTaskProductPresentationOptions():Promise<ProjectTaskProductPresentationOptions>{
 return productHostForHost((await headers()).get('host'))==='work'?{strictProduct:true}:{};
}
