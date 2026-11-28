// App-owned entry; shared payloads do not import another application.
import "./globals.css";
export { generateMetadata, default } from "@erpx/platform-workplace/routes/layout";
export const dynamic = "force-dynamic";
