// App-owned host composition.
import ProductEntryPage from "@erpx/workplace/entries/workspace";
export const dynamic = "force-dynamic";
export default function ServiceProductEntryPage(props: PageProps<"/product-entry">) {
  return ProductEntryPage(props);
}
