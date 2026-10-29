import {AppsHomeLink} from "@erpx/workplace/components/product-link-context";
import {cookies,headers} from 'next/headers';
import {notFound,redirect} from 'next/navigation';
import {configuredProductHandoff,productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {preferredAccountLanguage} from '@erpx/server-identity_access/public/account-preference-snapshot';
import {adminWorkspaceTarget,adminWorkRoleChoices,adminLaunchPath} from '@erpx/server-identity_access/public/admin-workspace';
import {ApiError} from '@erpx/server-runtime/public/http';
import {ProductNavigationBoundary} from '@erpx/workplace/components/product-navigation-boundary';
import {AdminRoleChoiceForm} from '@erpx/feature-identity_access/components/admin-role-choice-form';
import {ProductBrand} from '@erpx/feature-brand/components/product-brand';
import {Button,Card,PageContent,Stack,InlineNotice} from '@erpx/ui/components/foundation';
import styles from '@erpx/workplace/components/product-shell.module.css';
export const dynamic='force-dynamic';
export default async function AdminWorkRolePage({searchParams}:{searchParams:Promise<{companyId?:string|string[];next?:string|string[];notice?:string|string[]}>}){
 const query=await searchParams,input=adminWorkspaceTarget(typeof query.companyId==='string'?query.companyId:null,typeof query.next==='string'?query.next:null);if(productHostForHost((await headers()).get('host'))!=='admin'||!input)notFound();
 const language=await preferredAccountLanguage((await cookies()).get('erpx_language')?.value==='th'?'th':'en'),th=language==='th';let data:Awaited<ReturnType<typeof adminWorkRoleChoices>>|undefined;
 try{data=await adminWorkRoleChoices(input);}catch(error){if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401)redirect('/sso/start?'+new URLSearchParams({next:adminLaunchPath(input)}));if(!(error instanceof ApiError&&[403,404].includes(error.status)))throw error;}
 if(data?.target&&query.notice!=='access-changed')redirect(adminLaunchPath(input));
 return <ProductNavigationBoundary language={language}><div className={styles.shell}><header className={styles.header}><AppsHomeLink className={styles.brand}><ProductBrand appearance="standard" variant="symbol" className={styles.mark}/><strong>ERPX Admin</strong></AppsHomeLink><nav className={styles.links}><a data-product-switch href={configuredProductHandoff('apps')!}>Apps</a><a data-product-switch href={configuredProductHandoff('account')!}>Account</a></nav></header><PageContent as="main"><Stack><h1>{th?'เลือกบทบาททำงาน':'Choose a work role'}</h1>{data?<><p>{data.company.name} · {input.kind==='management'?'ERPX Management':th?'ดูแลบริษัท':'Company Admin'}</p><p>{th?'เลือกบทบาทปัจจุบันที่ใช้เปิดพื้นที่นี้':'Choose a current role for this workspace.'}</p>{query.notice==='access-changed'&&<InlineNotice tone="warning">{th?'สิทธิ์หรือบทบาทเปลี่ยนไป โปรดตรวจรายการล่าสุด':'Your access or selected role changed. Review the current choices.'}</InlineNotice>}<div className={styles.grid}>{data.options.map(option=><Card appearance="standard" key={option.assignmentId}><AdminRoleChoiceForm><Stack><input type="hidden" name="companyId" value={input.companyId}/><input type="hidden" name="next" value={input.target}/><input type="hidden" name="assignmentId" value={option.assignmentId}/><input type="hidden" name="assignmentVersion" value={option.assignmentVersion}/><input type="hidden" name="revision" value={data.revision}/><strong>{option.label}</strong><div><Button appearance="standard" type="submit">{th?'ใช้บทบาทนี้':'Use this role'}</Button></div></Stack></AdminRoleChoiceForm></Card>)}</div></>:<InlineNotice tone="warning">{th?'ไม่มีบทบาทปัจจุบันที่เปิดหน้านี้ได้ โปรดเลือกพื้นที่จาก Apps':'No current role can open this page. Return to Apps to choose a workspace.'}</InlineNotice>}</Stack></PageContent></div></ProductNavigationBoundary>;
}
