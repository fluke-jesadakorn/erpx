import 'server-only';
import {headers} from 'next/headers';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import type {FinanceProductPresentationOptions} from './record-presentation';
/** Actual Server request selection only. This tightens product presentation;
 * Host is never an acting authority and owner policies remain independent. */
export async function financeProductPresentationOptions():Promise<FinanceProductPresentationOptions>{
 return productHostForHost((await headers()).get('host'))==='finance'?{strictProduct:true}:{};
}
