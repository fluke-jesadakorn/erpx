// App-owned entry; shared payloads do not import another application.
import "./globals.css";
import RootLayout from "@erpx/workplace/routes/layout";
export { generateMetadata } from "@erpx/workplace/routes/layout";
export const dynamic = "force-dynamic";
export default function ServiceRootLayout(props: LayoutProps<"/">) {
  return RootLayout(props);
}
