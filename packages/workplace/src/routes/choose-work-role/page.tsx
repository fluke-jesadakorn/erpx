import {productAccessLabel} from '@erpx/feature-work_projection/lib/product-access-label';
import { preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { cookies, headers } from "next/headers";
import { notFound, redirect } from "next/navigation";
import Link from "next/link";
import { configuredProductHandoff, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { product } from "@erpx/server-commercial/public/product-registry";
import { productWorkspaceTarget, productWorkRoleChoices, productLaunchPath } from "@erpx/server-identity_access/public/product-workspace";
import { ApiError } from "@erpx/server-runtime/public/http";
import { ProductNavigationBoundary } from "@erpx/workplace/components/product-navigation-boundary";
import { ProductBrand } from "@erpx/feature-brand/components/product-brand";
import { Button, Card, PageContent, Stack, InlineNotice } from "@erpx/ui/components/foundation";
import styles from "@erpx/workplace/components/product-shell.module.css";

export const dynamic = "force-dynamic";
export default async function ProductWorkRolePage({ searchParams }: {
  searchParams: Promise<{ companyId?: string | string[]; next?: string | string[]; notice?: string | string[] }>;
}) {
  const audience = productHostForHost((await headers()).get("host"));
  const query = await searchParams;
  const definition = audience && product(audience);
  const input = audience && productWorkspaceTarget(audience, typeof query.companyId === "string" ? query.companyId : null,
    typeof query.next === "string" ? query.next : null);
  if (!audience || !definition || !input) notFound();
  const language = await preferredAccountLanguage((await cookies()).get("erpx_language")?.value === "th" ? "th" : "en"), th = language === "th";
  let data: Awaited<ReturnType<typeof productWorkRoleChoices>> | undefined;
  try { data = await productWorkRoleChoices(audience, input); }
  catch (error) {
    if (error instanceof Error && error.message === "UNAUTHENTICATED" || error instanceof ApiError && error.status === 401)
      redirect("/sso/start?" + new URLSearchParams({ next: productLaunchPath(input) }));
    if (!(error instanceof ApiError && [403, 404].includes(error.status))) throw error;
  }
  if (data?.target && query.notice !== "access-changed") redirect(productLaunchPath(input));
  return <ProductNavigationBoundary language={language}><div className={styles.shell}>
    <header className={styles.header}>
      <Link href="/" className={styles.brand} prefetch={false}><ProductBrand appearance="standard" variant="symbol" className={styles.mark} />
        <strong>ERPX {definition.label[language]}</strong></Link>
      <nav className={styles.links}><a data-product-switch href={configuredProductHandoff("apps")!}>Apps</a><a data-product-switch href={configuredProductHandoff("account")!}>Account</a></nav>
    </header>
    <PageContent as="main"><Stack>
      <h1>{th ? "เลือกบทบาททำงาน" : "Choose a work role"}</h1>
      {data ? <>
        <p>{data.company.name} · {definition.label[language]}</p>
        <p>{th ? "เลือกบทบาทที่จะใช้ในพื้นที่ทำงานนี้" : "Choose the role to use in this workspace."}</p>
        {query.notice === "access-changed" && <InlineNotice tone="warning">{th ? "สิทธิ์หรือบทบาทเปลี่ยนไป โปรดตรวจรายการล่าสุดและเลือกอีกครั้ง" : "Your access or selected role changed. Review the current choices and choose again."}</InlineNotice>}
        <div className={styles.grid}>{data.options.map(option => <Card appearance="standard" key={option.assignmentId}>
          <form method="post" action="/launch"><Stack>
            <input type="hidden" name="companyId" value={input.companyId} />
            <input type="hidden" name="next" value={input.target} />
            <input type="hidden" name="assignmentId" value={option.assignmentId} />
            <input type="hidden" name="assignmentVersion" value={option.assignmentVersion} />
            <input type="hidden" name="revision" value={data.revision} />
            <strong>{option.label}</strong>
            {productAccessLabel(option,th?"th":"en")&&<span>{productAccessLabel(option,th?"th":"en")}</span>}
            <div><Button appearance="standard" type="submit">{th ? "ใช้บทบาทนี้" : "Use this role"}</Button></div>
          </Stack></form>
        </Card>)}</div>
      </> : <InlineNotice tone="warning">{th ? "ไม่มีบทบาทและแพ็กเกจที่พร้อมใช้สำหรับหน้านี้ โปรดกลับไปเลือกพื้นที่ทำงาน" : "No current role and package can open this page. Return to Apps to choose a workspace."}</InlineNotice>}
    </Stack></PageContent>
  </div></ProductNavigationBoundary>;
}
