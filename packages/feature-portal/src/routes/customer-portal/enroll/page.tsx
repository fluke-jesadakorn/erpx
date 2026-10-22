import { CustomerPortalEnrollment } from "@erpx/feature-customer_authority/components/customer-portal-enrollment";

export const dynamic = "force-dynamic";
export const metadata = { title: "Accept Customer Portal invitation", referrer: "no-referrer" };

export default async function CustomerPortalEnrollmentPage({searchParams}:{searchParams:Promise<Record<string,string|string[]|undefined>>}) {
  const query=await searchParams;
  return <CustomerPortalEnrollment verifiedReturn={query.verified==='1'}/>;
}
