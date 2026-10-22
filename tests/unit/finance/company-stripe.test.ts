import {describe,expect,it} from 'vitest';
import {currentStripeOnboardingUrl} from "@erpx/feature-billing/lib/company-stripe";

const connection={account_id:'acct_current',livemode:false};
const now=1790832000000;
const job=(payload:Record<string,unknown>,state='completed',kind='connect_refresh')=>({state,kind,payload_json:JSON.stringify({accountId:'acct_current',livemode:false,url:'https://connect.stripe.com/setup/current',urlExpiresAt:now+60000,...payload})});

describe('Company Stripe setup links',()=>{
 it('does not send a reconnected company to the previous merchant',()=>{
  expect(currentStripeOnboardingUrl(connection,[job({accountId:'acct_previous',url:'https://connect.stripe.com/setup/previous'})],now)).toBeNull();
  expect(currentStripeOnboardingUrl(connection,[job({accountId:'acct_previous'}),job({})],now)).toBe('https://connect.stripe.com/setup/current');
 });
 it('rejects a different mode, expired links and unverified historical links',()=>{
  expect(currentStripeOnboardingUrl(connection,[job({livemode:true}),job({urlExpiresAt:now}),{kind:'connect',state:'completed',payload_json:JSON.stringify({url:'https://connect.stripe.com/setup/legacy'})}],now)).toBeNull();
 });
 it('ignores unfinished jobs, unrelated jobs and malformed or off-site URLs',()=>{
  expect(currentStripeOnboardingUrl(connection,[job({},'pending'),job({},'completed','event'),job({url:'https://connect.stripe.com.attacker.test/setup'}),job({url:'http://connect.stripe.com/setup'}),job({url:'https://user@connect.stripe.com/setup'}),{kind:'connect',state:'completed',payload_json:'broken'}],now)).toBeNull();
 });
});
