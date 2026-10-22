import {cookies,headers} from 'next/headers';
import {notFound,redirect} from 'next/navigation';
import Link from 'next/link';
import {currentProviderContext} from '@erpx/server-identity_access/public/provider-context';
import {ProviderContextBoundary} from '@erpx/feature-identity_access/components/provider-context-boundary';
import {ProviderContextSwitcher} from '@erpx/feature-identity_access/components/provider-context-switcher';
import {providerEntryOverview} from '@erpx/server-identity_access/public/provider-entry';
import {preferredAccountLanguage} from '@erpx/server-identity_access/public/account-preference-snapshot';
import {adminLaunchChoices} from '@erpx/server-work_projection/public/admin-launcher';
import {configuredProductHandoff,productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {ApiError} from '@erpx/server-runtime/public/http';
import {ProductNavigationBoundary} from '@erpx/workplace/components/product-navigation-boundary';
import {ProductBrand} from '@erpx/feature-brand/components/product-brand';
import {PageContent,Stack,InlineNotice} from '@erpx/ui/components/foundation';
import styles from '@erpx/workplace/components/product-shell.module.css';

export const dynamic='force-dynamic';
export default async function ProviderEntryPage(){
 if(productHostForHost((await headers()).get('host'))!=='admin')notFound();
 const language=await preferredAccountLanguage((await cookies()).get('erpx_language')?.value==='th'?'th':'en'),th=language==='th';
 let overview:Awaited<ReturnType<typeof providerEntryOverview>>;
 try{overview=await providerEntryOverview();}
 catch(error){if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/sso/start?next=%2Fprovider');throw error;}
 const providerContext=overview?await currentProviderContext():null;
 if(overview&&providerContext&&overview.assignmentId!==providerContext.selectedId)throw new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Reload Administration to review the current Platform role.');
 const management=overview?(await adminLaunchChoices()).filter(choice=>choice.kind==='management'):[];
 return <ProductNavigationBoundary language={language}><div className={styles.shell}>
  <header className={styles.header}><Link data-product-switch href="/" prefetch={false} className={styles.brand}><ProductBrand appearance="standard" variant="symbol" className={styles.mark}/><strong>ERPX Administration</strong></Link>
   <nav className={styles.links}><a data-product-switch href={configuredProductHandoff('apps')!}>Apps</a><a data-product-switch href={configuredProductHandoff('account')!}>Account</a></nav></header>
  <PageContent as="main"><Stack><h1>{th?'พื้นที่ผู้ให้บริการ':'Provider administration'}</h1>
   {overview&&providerContext?<ProviderContextBoundary initial={providerContext}><ProviderContextSwitcher/>
    <p>{overview.name} · {overview.role}</p>
    <div className={styles.grid}>
     {overview.commercial&&<Link data-product-switch href="/provider/commercial" prefetch={false} className={styles.tile}><strong>{th?'ตรวจย้ายสิทธิ์และแพ็กเกจบริษัท':'Company access migration and packages'}</strong><span>{th?'Platform Commercial':'Platform Commercial'}</span></Link>}
     {management.map(choice=><a data-product-switch href={choice.href} key={choice.href} className={styles.tile}><strong>ERPX Management</strong><span>{choice.company}</span><span className={styles.muted}>{choice.workspace}</span></a>)}
    </div>
    {!overview.commercial&&!management.length&&<InlineNotice tone="info">{th?'บทบาท Platform ปัจจุบันไม่มีเครื่องมือจัดการแพ็กเกจหรือ ERPX Management':'Your current Platform role has no package administration or ERPX Management entry.'}</InlineNotice>}
   </ProviderContextBoundary>:<InlineNotice tone="warning">{th?'ต้องใช้สิทธิ์ Platform ปัจจุบันเพื่อเข้าพื้นที่ผู้ให้บริการ':'Current Platform authority is required to enter provider administration.'}</InlineNotice>}
  </Stack></PageContent>
 </div></ProductNavigationBoundary>;
}
