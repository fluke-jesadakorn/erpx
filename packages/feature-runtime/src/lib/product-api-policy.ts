import { modulePlacement, product, type ProductHostKey } from "@erpx/server-commercial/public/rules/product-registry";

const ownerResources: Partial<Record<ProductHostKey, readonly string[]>> = {
  admin: ["agents", "ai-providers", "authority", "beta", "billing", "brand", "branding", "business-readiness", "feature-catalogue", "iam", "technology", "usage"],
  work: ["approvals", "calendar", "collaboration", "daily-plan", "executive", "feed", "handbook", "handover", "inbox", "messaging", "operations", "posts", "routine-schedules", "routine-templates", "staff", "staff-tasks", "stories", "work"],
  finance: ["finance", "expenses", "hospitality-tax-documents"],
  people: ["benefits", "people", "leave", "leave-requests", "overtime-requests"],
  sales: ["crm", "customer-access", "customer-duplicates", "customer-followups", "customer-identity", "customer-notes", "customers", "ecommerce", "marketing"],
  supply: ["landed-costs", "procurement-rfqs", "quality", "stock-counts", "stock-transfers", "stock-transit", "supplier-bills", "supplier-promises"],
  assets: ["assets", "maintenance"],
  projects: ["projects"],
  recruit: ["talent"],
  learn: ["learning"],
  support: ["support", "service-teams"],
  service: ["field-service"],
  legal: ["legal"],
  manufacturing: ["production"],
};

function holdingResource(pathname: string): string | null {
  const match = /^\/api\/v1\/holdings\/[^/]+\/(.*)$/.exec(pathname);
  if (!match) return null;
  let parts = match[1].split("/");
  if (parts.at(-1) === "") parts.pop();
  // App Router decodes dynamic segments before the controller sees them.
  // Apply ownership to those same segments, including encoded family names.
  try { parts = parts.map(part => decodeURIComponent(part)); } catch { return null; }
  if (parts.some(part => !part || part === "." || part === ".." || part.includes("/") || /%(?:2f|5c)/i.test(part) || /[\\\u0000-\u001f]/.test(part))) return null;
  if (parts[0] === "branches") {
    if (!parts[1]) return null;
    parts = parts.slice(2);
  }
  if (parts[0] === "departments") {
    if (!parts[1]) return null;
    parts = parts.slice(2);
  }
  return parts.length ? parts.join("/") : null;
}

/** POS startup is a closed Front transport; null means another resource. */
export function posStartupApiAllowed(host:ProductHostKey,pathname:string,method:string):boolean|null{
 const resource=holdingResource(pathname);
 if(resource===null)return pathname.startsWith("/api/v1/holdings/")?false:null;
 if(resource!=="pos/startup"&&!resource?.startsWith("pos/startup/"))return null;
 if(host!=="pos")return false;
 if(resource==="pos/startup")return method==="GET"||method==="HEAD";
 return method==="POST"&&["charts","accounts","periods","rates","mapping","warehouses","products"].some(action=>resource===`pos/startup/${action}`);
}

/** Initial POS stock is an original physical Count with a named source. */
export function posInitialPhysicalStockApiAllowed(host: ProductHostKey, pathname: string, method: string): boolean | null {
  const resource = holdingResource(pathname);
  if (resource === null) return pathname.startsWith("/api/v1/holdings/") ? false : null;
  if (resource !== "pos/initial-physical-stock" && !resource?.startsWith("pos/initial-physical-stock/")) return null;
  if (host !== "pos") return false;
  const parts = resource.split("/"), read = method === "GET" || method === "HEAD";
  if (parts.length === 2) return read || method === "POST";
  if (parts[2] === "masters") return false;
  return read && (parts.length === 3 || parts.length === 4 && parts[3] === "flow")
    || method === "POST" && parts.length === 4 && ["submit", "approve", "post", "reject", "cancel", "flow"].includes(parts[3]);
}

/** Office transport ownership, in addition to the original session, Company,
 * IAM, purchase and record checks in each controller. This never forwards a
 * request or turns an included capability into its owner's management UI. */
export function officeApiAllowed(host: ProductHostKey, pathname: string, method: string) {
  if (host !== "admin" && product(host)?.shell !== "office") return false;
  const read = method === "GET" || method === "HEAD";
  if (pathname === "/api/auth/get-session") return read;
  if (pathname === "/api/auth/sign-out") return method === "POST";
  // Commercial step-up verifies the current Admin session without leaving
  // its captured review. Enrollment, recovery and Account settings stay central.
  if (pathname === "/api/v1/account/iam/mfa/verify") return host === "admin" && method === "POST";
  if (pathname === "/api/v1/workspace-preference") return method === "POST";
  if (pathname === "/api/v1/account/work-context") return read;
  if (/^\/api\/v1\/me\/page-layouts\/[^/]+$/.test(pathname)) return read || method === "POST";
  // Company Executive work and its existing scoped MCP connection live on Work.
  // Provider Executive remains on the distinct Administration platform API.
  if (pathname === "/api/mcp/executive") return host === "work";
  const resource = holdingResource(pathname);
  if (!resource) return false;
  const [root, second, third] = resource.split("/");
  const expenseDecoded=(value:string|undefined)=>{try{return value===undefined?null:decodeURIComponent(value);}catch{return null;}};
  const expenseOwner=expenseDecoded(root),expenseFamily=expenseDecoded(second);
  if(expenseOwner==='finance'&&expenseFamily?.startsWith('expense-setup')){
    if(root!==expenseOwner||second!==expenseFamily||second!=='expense-setup')return false;
    const parts=resource.split('/');
    return host==='finance'&&(parts.length===2&&read||parts.length===3&&method==='POST'&&['charts','accounts','periods','rates','confirmations','mapping'].includes(parts[2]));
  }


  // The Helpdesk Portal uses its own reviewed ticket binding. The general
  // Support family cannot broaden this named transport's verbs or actions.
  if (root === "support" && second === "ticket-portal") {
    const parts = resource.split("/");
    return host === "support" && (
      parts.length === 2 && (read || method === "POST")
      || parts.length === 3 && read
      || parts.length === 4 && ["decisions", "responses"].includes(parts[3]) && method === "POST"
    );
  }

  // These are shared transports, with the original target/byte/role policy.
  if (/^(?:navigation|search|assignments\/active|companies\/active|people-public-directory|record-details)$/.test(resource)) return true;
  if (["attachment-links", "attachment-sets", "attachments", "uploads", "assistant", "push", "page-layouts", "media-publications", "profiles"].includes(root)) return true;
  if (root === "money") return resource === "money" && read
    || ["money/preference", "money/convert"].includes(resource) && method === "POST"
    || host === "finance";
  // Shared Work summaries, personal intake and source review retain their
  // native controllers; owner management collections below remain separate.
  if (root === "work" || root === "requests" || root === "self-service") return true;
  if (root === "calendar") return read || host === "work"
    || host === "people" && method === "POST" && ["calendar", "calendar/tasks", "calendar/shifts"].includes(resource);
  if (root === "routing-attention") return host === "admin" || host === "work";
  if (root === "records") {
    if (resource === "records/search") return read;
    const owner = second && Object.hasOwn(modulePlacement, second) ? modulePlacement[second as keyof typeof modulePlacement] : undefined;
    if (owner === host || host === "admin" && (owner === "provider" || second === "agent")) return true;
    if (host === "sales" && ["customer_360", "customer_authority"].includes(second)) return true;
    // Work's personal retained leave source is not the People collection.
    return host === "work" && second === "people_payroll" && !!third && read;
  }
  if (root === "flows") {
    if (!third || resource.split("/").length !== 3) return false;
    if (["request", "staff_task", "expense", "leave"].includes(second)) return true;
    if (["stock_count", "transfer", "order"].includes(second)) return host === "supply" || second === "order" && host === "sales";
    if (["invoice", "journal"].includes(second)) return host === "finance";
    if (second === "payroll") return host === "people";
    // Generic record flows resolve the actual native target in their owner.
    return second === "record";
  }
  // The People controller decodes App Router segments once more. Residual
  // encoding of this reserved family must not fall into generic People access.
  if(root==='people'){
    const decoded=(part:string|undefined)=>{try{return part===undefined?undefined:decodeURIComponent(part);}catch{return undefined;}};
    const payroll=decoded(second),setup=decoded(third);
    if(payroll==='payroll'&&setup?.startsWith('posting-prerequisites')&&(second!==payroll||third!==setup))return false;
  }
  // This exact Payroll setup does not open the general Finance collections.
  if(root==='people'&&second==='payroll'&&third?.startsWith('posting-prerequisites')){
    const parts=resource.split('/');
    return host==='people'&&third==='posting-prerequisites'&&(parts.length===3&&read||parts.length===4&&method==='POST'&&['charts','accounts','periods','rates','confirmations'].includes(parts[3]));
  }
  // Exact Payroll liabilities remain inside People with their own source;
  // they never grant the general Finance bank collection or extra verbs.
  if(root==='people'&&second==='payroll'&&third==='runs'){
    const parts=resource.split('/');
    if(['other-liabilities','other-remittances'].includes(parts[4]??'')||['other-liabilities','other-remittances'].includes(parts[3]??''))
      return host==='people'&&!!parts[3]&&(parts.length===5&&parts[4]==='other-liabilities'&&read||parts[4]==='other-remittances'&&method==='POST'&&(parts.length===5||parts.length===6&&parts[5]==='bank-evidence'||parts.length===7&&parts[5]==='bank-evidence'&&parts[6]==='ignore'));
  }
  // Exact instruction bank evidence retains its closed Finance command route.
  // General Finance ownership cannot broaden its verbs or suffix actions.
  if(root==="finance" && second==="refund-instructions"){
    const parts=resource.split("/");
    if(parts[3]==="bank-evidence" || parts.length===3 && third==="bank-evidence")
      return host==="finance" && method==="POST" && parts.length===4 && parts[3]==="bank-evidence";
  }
  // The included initial physical Count is a closed owner constructor, not
  // the Supply Count collection or a new manual adjustment entitlement.
  if(root==="stock-counts" && second==="initial-physical"){
    const parts=resource.split("/");
    return host==="sales" && (
      read && (parts.length<=3 || parts.length===4 && parts[3]==="flow")
      || method==="POST" && (parts.length===2 || parts.length===4 && ["submit","approve","post","reject","cancel","flow"].includes(parts[3]))
    );
  }
  // Included Finance setup is closed before the general Production family.
  // No other host, mutation verb or arbitrary child inherits these transports.
  if(root==="production"&&second==="posting-setup"){
    const parts=resource.split("/");
    return host==="manufacturing" && (
      parts.length===2 && (read||method==="POST")
      || parts.length===3 && third==="existing" && read
      || parts.length===4 && third==="existing" && parts[3]==="confirm" && method==="POST"
      || parts.length===3 && third==="prerequisites" && read
      || parts.length===3 && ["charts","accounts","periods","rates"].includes(third) && method==="POST"
      || parts.length===3 && third==="profiles" && (read||method==="POST")
      || parts.length===5 && third==="profiles" && parts[4]==="decide" && method==="POST"
    );
  }
  if(root==="production"&&second==="initial-physical-stock"){
    const parts=resource.split("/");
    if(third==="masters")return host==="manufacturing"&&method==="POST"&&parts.length===4&&["products","warehouses"].includes(parts[3]);
    return host==="manufacturing"&&(
      read&&(parts.length<=3||parts.length===4&&parts[3]==="flow")
      ||method==="POST"&&(parts.length===2||parts.length===4&&["submit","approve","post","reject","cancel","flow"].includes(parts[3]))
    );
  }
  // Included Learning worker preparation is a closed named source, never
  // the general People Directory, lifecycle, account or access collection.
  if(root==="people"&&second==="learning-worker-startup"){
    if(host!=="learn")return false;
    const parts=resource.split("/");
    if(parts.length===2)return read;
    if(parts.length===3)return ["accounts","people"].includes(third)&&read||third==="prepare"&&method==="POST";
    if(["accounts","people","prepare"].includes(third))return false;
    return method==="POST"&&(parts.length===5&&parts[3]==="placement"&&["preview","submit"].includes(parts[4])
      ||parts.length===6&&parts[3]==="placements"&&parts[5]==="cancel");
  }
  // Included Projects worker preparation is a closed named source, never
  // the general People Directory, lifecycle, account or access collection.
  if(root==="people"&&second==="project-worker-startup"){
    if(host!=="projects")return false;
    const parts=resource.split("/");
    if(parts.length===2)return read;
    if(parts.length===3)return ["accounts","people"].includes(third)&&read||third==="prepare"&&method==="POST";
    if(["accounts","people","prepare"].includes(third))return false;
    return method==="POST"&&(parts.length===5&&parts[3]==="placement"&&["preview","submit"].includes(parts[4])
      ||parts.length===6&&parts[3]==="placements"&&parts[5]==="cancel");
  }
  // Service customer setup is not the general Finance party collection.
  if(root==="field-service"&&second==="customer-preparation"){
    const parts=resource.split("/");
    return host==="service"&&(parts.length===2&&(read||method==="POST")||parts.length===3&&third==="confirmations"&&method==="POST");
  }
  if (root === "field-service" && second === "parts-setup") {
    if (host !== "service") return false;
    const parts = resource.split("/");
    if (parts.length === 2) return read || method === "POST";
    if (parts.length === 3) return ["prerequisites", "stock"].includes(third) && read
      || ["existing", "charts", "accounts", "periods", "rates"].includes(third) && method === "POST";
    return parts.length === 4 && third === "masters" && ["products", "warehouses"].includes(parts[3]) && method === "POST";
  }
  if (root === "field-service" && second === "initial-physical-stock") {
    if (host !== "service") return false;
    const parts = resource.split("/");
    if (parts.length === 2) return read || method === "POST";
    if (third === "masters") return false;
    return read && (parts.length === 3 || parts.length === 4 && parts[3] === "flow")
      || parts.length === 4 && method === "POST" && ["submit", "approve", "post", "reject", "cancel", "flow"].includes(parts[3]);
  }
  // The Assets product continues the original Finance asset commands through
  // this closed transport. Reserved constructor names cannot become record IDs.
  if (root === "assets" && second === "financial-assets") {
    if (host !== "assets") return false;
    const parts = resource.split("/");
    if (parts.length === 2) return read || method === "POST";
    if (third === "prerequisites") return parts.length === 3 && read;
    if (["charts", "accounts", "periods", "rates"].includes(third))
      return parts.length === 3 && method === "POST";
    if (third === "disposals")
      return parts.length === 5 && method === "POST" && ["post", "cancel"].includes(parts[4]);
    return parts.length === 3 && read
      || parts.length === 4 && method === "POST" && ["edit", "cancel", "recognize", "disposals"].includes(parts[3]);
  }
  if (ownerResources[host]?.includes(root)) return true;

  // Published dependencies used by the existing same-page owner controls.
  if (root === "expenses") return host === "work";
  if (root === "staff-tasks") return host === "projects";
  if (root === "leave-requests" || root === "overtime-requests") return host === "work";
  if (root === "leave") return host === "work" && read && ["leave/calendar", "leave/summary"].includes(resource);
  if (root === "iam") return host === "people"; // Existing People access explorer.
  if (root === "people") return host === "recruit" && (resource === "people/hire-handoffs/consume" && method === "POST"
      || /^people\/hire-handoffs\/[^/]+\/pay-groups$/.test(resource) && (read || method === "POST"))
    || host === "admin" && (/^(?:people\/organization|people\/templates|people\/lifecycle\/placements)(?:\/|$)/.test(resource)
      || /^people\/lifecycle\/position\/(?:preview|bind)$/.test(resource));
  if (root === "reports") return host === "sales" && resource === "reports/crm-marketing";
  if (root === "finance") return host === "sales" && /^finance\/sales(?:\/|$)/.test(resource)
    || host === "projects" && /^finance\/project-cost-(?:sources|allocations)(?:\/|$)/.test(resource);
  if (root === "commerce") {
    if (second === "sales-service-products") return host === "sales" && resource === "commerce/sales-service-products" && (read || method === "POST");
    if (second === "sales-goods-posting-setup") return host === "sales" && resource === "commerce/sales-goods-posting-setup" && (read || method === "POST");
    if (second === "sales-posting-setup") return host === "sales" && (
      resource === "commerce/sales-posting-setup" && read
      || /^commerce\/sales-posting-setup\/(?:charts|accounts|periods|rates|defaults)$/.test(resource) && method === "POST"
      || /^commerce\/sales-posting-setup\/defaults\/[^/]+\/decide$/.test(resource) && method === "POST"
    );
    // Initial Supply posting configuration has its own closed transport; a
    // shared Commerce family never grants another product this setup action.
    if (second === "posting-setup") return host === "supply" && (
      resource === "commerce/posting-setup" && (read || method === "POST")
      || resource === "commerce/posting-setup/prerequisites" && read
      || /^commerce\/posting-setup\/(?:charts|accounts|periods|rates)$/.test(resource) && method === "POST"
    );
    if (host === "sales" || host === "supply") return true;
    return host === "finance" && /^commerce\/products(?:\/|$)/.test(resource);
  }
  if (resource === "hospitality/setup") return host === "finance" && read;
  // Finance's existing Hotel setup and transfer verification share the PMS
  // command endpoint. The controller additionally restricts the operation.
  if (resource === "pms") return host === "finance" && method === "POST";
  if (root === "pos") return host === "admin" && method === "POST" && /^pos\/connection\/(?:create|refresh|oauth)$/.test(resource)
    || host === "finance" && method === "POST" && /^pos\/(?:catalog\/save|register\/save|settings\/bootstrap|table\/create)$/.test(resource);
  return false;
}

/** Body selector for the intentionally shared Finance Hotel setup endpoint.
 * All normal product/legacy PMS callers retain the original PMS policy. */
export function pmsProductOperationAllowed(host: ProductHostKey | null, operation: unknown) {
  return host !== "finance" || typeof operation === "string" && [
    "payment.verify_transfer", "accounting.bootstrap", "property.create", "property.save", "room_type.save", "room.save", "rate.save",
  ].includes(operation);
}

/** People uses the original Calendar transport for clearance and roster work;
 * it cannot acquire the manual appointment/announcement editor through it. */
export function calendarProductActionAllowed(host: ProductHostKey | null, action: unknown) {
  return host !== "people" || typeof action === "string" && ["task.plan", "task.complete", "shift.reschedule"].includes(action);
}

/** Verified-account Support tickets have a distinct closed public transport. */
export function supportTicketPortalApiAllowed(host: ProductHostKey, pathname: string, method: string): boolean | null {
  const match = /^\/api\/customer-portal\/([^/]+)\/(.*)$/.exec(pathname);
  if (!match) return null;
  let parts = [match[1], ...match[2].split("/")];
  if (parts.at(-1) === "") parts.pop();
  try { parts = parts.map(part => decodeURIComponent(part)); } catch { return false; }
  if (parts.some(part => !part || part === "." || part === ".." || part.includes("/") || /%(?:2f|5c)/i.test(part) || /[\\\u0000-\u001f]/.test(part))) return false;
  parts = parts.slice(1);
  if (parts[0] !== "tickets") return null;
  if (host !== "portal") return false;
  const read = method === "GET" || method === "HEAD";
  return parts.length === 1 && read
    || parts.length === 3 && parts[2] === "acknowledgements" && method === "POST"
    || parts.length === 5 && parts[2] === "files" && parts[4] === "content" && read;
}
