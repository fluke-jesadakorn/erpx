import { headers } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { configuredProductOrigin, productHostForHost } from "@erpx/platform-runtime-ui/lib/product-hosts";
import { createAuth } from "@erpx/identity-access-server/public/auth";
import { AccountHome } from "@erpx/identity-access-ui/components/account-home";

export default async function AccountEntry() {
  const requestHeaders = await headers();
  if (productHostForHost(requestHeaders.get("host")) !== "account") notFound();
  const session = await (await createAuth(configuredProductOrigin("account")!)).api.getSession({ headers: requestHeaders });
  if (!session) redirect("/login");
  return <AccountHome />;
}
