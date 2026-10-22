import { accountSelfLoginHref, accountSelfPageReturn, type EntryPageSearchParams } from '@erpx/server-identity_access/public/rules/page-entry-return';
import {notFound,redirect} from 'next/navigation';
import {headers} from 'next/headers';
import {configuredProductOrigin,productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {AccountHome} from '@erpx/feature-identity_access/components/account-home';
import {currentSession} from '@erpx/server-identity_access/public/authorization';

export const dynamic='force-dynamic';
export default async function MyProfilePage({ searchParams }: { searchParams?: Promise<EntryPageSearchParams> }){
 const origin=configuredProductOrigin('account');if(!origin)notFound();
 if(productHostForHost((await headers()).get('host'))!=='account')redirect(new URL(accountSelfPageReturn('/my-profile', await searchParams ?? {}),origin).href);
 await currentSession().catch(async()=>redirect(accountSelfLoginHref('/my-profile', await searchParams ?? {})));
 return <AccountHome/>;
}
