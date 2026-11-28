import {AppsHomeLink} from "@erpx/platform-workplace/components/product-link-context";
import {ProductAccountTrigger} from "@erpx/platform-workplace/components/product-account-provider";
import {AppsSidebar} from "@erpx/platform-workplace/components/apps-sidebar";
import {cookies,headers} from 'next/headers';
import {notFound,redirect} from 'next/navigation';
import Link from 'next/link';
import {currentProviderContext} from '@erpx/identity-access-server/public/provider-context';
import {ProviderContextBoundary} from '@erpx/identity-access-ui/components/provider-context-boundary';
import {ProviderContextSwitcher} from '@erpx/identity-access-ui/components/provider-context-switcher';
import {providerEntryOverview} from '@erpx/identity-access-server/public/provider-entry';
import {preferredAccountLanguage} from '@erpx/identity-access-server/public/account-preference-snapshot';
import {adminLaunchChoices} from '@erpx/work-projection-server/public/admin-launcher';
import {configuredProductHandoff,productHostForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {ProductNavigationBoundary} from '@erpx/platform-workplace/components/product-navigation-boundary';
import {ProductBrand} from '@erpx/brand-ui/components/product-brand';
import {PageContent,Stack,InlineNotice} from '@erpx/platform-ui/components/foundation';
import styles from '@erpx/platform-workplace/components/product-shell.module.css';

export const dynamic='force-dynamic';
export default async function ProviderEntryPage(){
 if(productHostForHost((await headers()).get('host'))!=='admin')notFound();
 const language=await preferredAccountLanguage((await cookies()).get('erpx_language')?.value==='th'?'th':'en'),th=language==='th';
 let overview:Awaited<ReturnType<typeof providerEntryOverview>>;
 try{overview=await providerEntryOverview();}
 catch(error){if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/sso/start?next=%2Fprovider');throw error;}
 const providerContext=overview?await currentProviderContext():null;
 if(overview&&providerContext&&overview.assignmentId!==providerContext.selectedId)throw new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Reload Administration to review the current Platform role.');
 const roleLabel=providerContext?.choices.find(choice=>choice.id===providerContext.selectedId)?.label;
 const management=overview?(await adminLaunchChoices()).filter(choice=>choice.kind==='management'):[];
 return <ProductNavigationBoundary language={language}><div className={styles.shell}>
  <header className={styles.header}><AppsHomeLink className={styles.brand}><ProductBrand appearance="standard" variant="symbol" className={styles.mark}/><strong>ERPX Administration</strong></AppsHomeLink>
   <nav className={styles.links}><a data-product-switch href={configuredProductHandoff('apps')!}>Apps</a><a data-product-switch href={configuredProductHandoff('account')!}>Account</a></nav><ProductAccountTrigger roleLabel={roleLabel} language={language}/></header>
  <div className={styles.body}><AppsSidebar language={language} roleLabel={roleLabel} items={[{id:"admin",label:"Admin",href:"/",groupId:"administration"},{id:"provider",label:"Platform / Provider",href:"/provider",groupId:"administration"}]}/><PageContent as="main"><Stack><h1>{th?'พื้นที่ผู้ให้บริการ':'Provider administration'}</h1>
   {overview&&providerContext?<ProviderContextBoundary initial={providerContext}><ProviderContextSwitcher/>
    <p>{overview.name} · {overview.role}</p>
    <div className={styles.grid}>
     {overview.commercial&&<Link data-product-switch href="/provider/commercial" prefetch={false} className={styles.tile}><strong>{th?'ตรวจย้ายสิทธิ์และแพ็กเกจบริษัท':'Company access migration and packages'}</strong><span>{th?'Platform Commercial':'Platform Commercial'}</span></Link>}
     {management.map(choice=><a data-product-switch href={choice.href} key={choice.href} className={styles.tile}><strong>ERPX Management</strong><span>{choice.company}</span><span className={styles.muted}>{choice.workspace}</span></a>)}
    </div>
    {!overview.commercial&&!management.length&&<InlineNotice tone="info">{th?'บทบาท Platform ปัจจุบันไม่มีเครื่องมือจัดการแพ็กเกจหรือ ERPX Management':'Your current Platform role has no package administration or ERPX Management entry.'}</InlineNotice>}
   </ProviderContextBoundary>:<InlineNotice tone="warning">{th?'ต้องใช้สิทธิ์ Platform ปัจจุบันเพื่อเข้าพื้นที่ผู้ให้บริการ':'Current Platform authority is required to enter provider administration.'}</InlineNotice>}
  </Stack></PageContent></div>
 </div></ProductNavigationBoundary>;
}
