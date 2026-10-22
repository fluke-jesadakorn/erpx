import {ApiError} from '@erpx/server-runtime/public/http';
import type {BetaEnvironment} from '@erpx/server-manage_erpx/public/beta-config';
import {createSandboxStripe} from '@erpx/server-billing/application/stripe';
export type {BillingStripe,StripeObject} from '@erpx/server-billing/domain/stripe';
export {stripeUrl,stripeList,testObject} from '@erpx/server-billing/domain/stripe';
export function billingStripe(env:BetaEnvironment){return createSandboxStripe({secretKey:()=>env.STRIPE_SECRET_KEY,error:(...args)=>new ApiError(...args),request:(path,fields,key)=>fetch(`https://api.stripe.com/v1/${path}`,{method:fields?'POST':'GET',headers:{Authorization:`Bearer ${env.STRIPE_SECRET_KEY}`,'Stripe-Version':'2025-02-24.acacia',...(fields?{'Content-Type':'application/x-www-form-urlencoded'}:{}),...(key?{'Idempotency-Key':key}:{})},body:fields?new URLSearchParams(fields):undefined,signal:AbortSignal.timeout(15000)})})}
