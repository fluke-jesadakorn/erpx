import 'server-only';
import {headers} from 'next/headers';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import type {FinanceProductPresentationOptions} from '@erpx/server-finance/adapters/record-presentation';
/** Actual Server request selection only. This tightens product presentation;
 * Host is never an acting authority and owner policies remain independent. */
export async function financeProductPresentationOptions():Promise<FinanceProductPresentationOptions>{
 return productHostForHost((await headers()).get('host'))==='finance'?{strictProduct:true}:{};
}
