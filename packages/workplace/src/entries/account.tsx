import { headers } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { configuredProductOrigin, productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { createAuth } from "@erpx/server-identity_access/public/auth";
import { AccountHome } from "@erpx/feature-identity_access/components/account-home";

export default async function AccountEntry() {
  const requestHeaders = await headers();
  if (productHostForHost(requestHeaders.get("host")) !== "account") notFound();
  const session = await (await createAuth(configuredProductOrigin("account")!)).api.getSession({ headers: requestHeaders });
  if (!session) redirect("/login");
  return <AccountHome />;
}
