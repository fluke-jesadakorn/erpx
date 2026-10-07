import type {WorkContext,currentSession,requireWorkContext,requirePlatformContext} from "../../identity_access/public/authorization";
import type {NavigationContext} from "@/lib/workplace-navigation";
import {allowedWidgets,pageLayoutSchema,isConfigurablePage,type LayoutSnapshot} from "@/lib/page-layouts";
import {pageLayoutInputSchema} from "../domain/page-layouts";
export type PageLayoutUnit<Effect>={companyAuthority(actor:WorkContext):Promise<boolean>;layouts(company:string,page:string,user:string,base:string):Promise<{owner_id:string;layout_json:string|null;version:number}[]>;lock(scope:string):Promise<unknown>;workGuards(actor:WorkContext,key:string):Effect[];iamGuards(actor:WorkContext,key:string):Effect[];clearAssertions(key:string):Effect;batch(effects:Effect[]):Promise<unknown>;currentSession(session:string,user:string):Promise<Record<string,unknown>|null>;currentPlatform(assignment:string,user:string,role:string,now:number,now2:number):Promise<Record<string,unknown>|null>;receipt(key:string):Promise<{request_hash:string;result_json:string}|null>;save(company:string,owner:string,base:string,page:string,value:string|null,version:number,now:number):Promise<unknown>;saveReceipt(key:string,hash:string,result:string,user:string,company:string,base:string,page:string,action:string,now:number):Promise<unknown>;};
export function createPageLayoutService<Effect>(ports:{error(status:number,code:string,message:string):Error;now():number;hash(value:string):string;currentSession:typeof currentSession;requireWorkContext:typeof requireWorkContext;requirePlatformContext:typeof requirePlatformContext;prepare(work:WorkContext|null):Promise<void>;unit():PageLayoutUnit<Effect>;navigation(actor:WorkContext):Promise<NavigationContext>;base(actor:WorkContext):string;transaction<T>(action:(tx:PageLayoutUnit<Effect>)=>Promise<T>):Promise<T>}) {
 return async function pageLayout(method:string,command:()=>Promise<{input:unknown;key:string}>,pageKey:string,holdingSlug?:string,account=false):Promise<LayoutSnapshot>{
    if (!isConfigurablePage(pageKey))
      throw ports.error(
        404,
        "PAGE_LAYOUT_UNKNOWN",
        "This page has no configurable widgets.",
      );
    const { session } = await ports.currentSession();
    if (!session)
      throw ports.error(
        401,
        "SIGN_IN_REQUIRED",
        "Sign in to arrange your workspace.",
      );
    const work = holdingSlug ? await ports.requireWorkContext(holdingSlug) : null;
    const platform = !work && !account ? await ports.requirePlatformContext() : null;
    await ports.prepare(work);const db=ports.unit();
    const context: NavigationContext = work
      ? await ports.navigation(work)
      : {
          workspaceBase: account ? "/account" : "/platform",
          scopeKind: "platform",
          role: platform?.role ?? "employee",
        };
    const company = work?.companyId ?? (account ? "account" : "platform"),
      base = work ? ports.base(work) : context.workspaceBase;
    if (
      work &&
      (pageKey.startsWith("platform-") || pageKey.startsWith("account-"))
    )
      throw ports.error(
        403,
        "LAYOUT_SCOPE",
        "This layout belongs to another workspace.",
      );
    if (
      (account && !pageKey.startsWith("account-")) ||
      (!account && !work && !pageKey.startsWith("platform-"))
    )
      throw ports.error(
        403,
        "LAYOUT_SCOPE",
        "This layout belongs to another workspace.",
      );
    const canPublish = work
      ? work.scope.kind === "holding" && (await db.companyAuthority(work))
      : !account && platform?.role === "platform_admin";
    const definitions = allowedWidgets(pageKey, context),
      ids = new Set(definitions.map((w) => w.id));
    if (!ids.size)
      throw ports.error(
        403,
        "PAGE_LAYOUT_FORBIDDEN",
        "This page is unavailable for your current role.",
      );
    const user = session.user.id;
    const read = async (
      tx: PageLayoutUnit<Effect>,
    ): Promise<LayoutSnapshot> => {
      const rows = await tx.layouts(company,pageKey,user,base);
      const personal = rows.find((r) => r.owner_id === user),
        organization = rows.find((r) => r.owner_id === "organization");
      const decode = (value: string | null | undefined) =>
        value
          ? {
              ...pageLayoutSchema.parse(JSON.parse(value)),
              widgets: pageLayoutSchema
                .parse(JSON.parse(value))
                .widgets.filter((w) => ids.has(w.id)),
            }
          : null;
      return {
        personal: decode(personal?.layout_json),
        organization: decode(organization?.layout_json),
        version: personal?.version ?? 0,
        organizationVersion: organization?.version ?? 0,
        canPublish: !!canPublish,
      };
    };
    if (method === "GET")
      return await read(db);
    const { input, key } = await command(),
      parsed = pageLayoutInputSchema.parse(input);
    if (parsed.action === "publish" && !canPublish)
      throw ports.error(
        403,
        "LAYOUT_PUBLISH_FORBIDDEN",
        "Company organization management is required to publish a layout.",
      );
    if (parsed.action !== "reset" && !parsed.layout)
      throw ports.error(422, "LAYOUT_REQUIRED", "Provide a layout.");
    if (parsed.layout?.widgets.some((w) => !ids.has(w.id)))
      throw ports.error(
        403,
        "WIDGET_UNAVAILABLE",
        "A widget is unavailable for your current role.",
      );
    const hash = ports.hash(
        JSON.stringify({
          user,
          company,
          base,
          pageKey,
          assignment: work?.assignmentId ?? platform?.assignmentId,
          input: parsed,
        }),
      );
    const result = await ports.transaction(async (tx) => {
      await tx.lock(`${company}:${base}:${pageKey}`);
      if (work) {
        await tx.batch([
          ...tx.workGuards(work,key),
          ...tx.iamGuards(work,key),
          tx.clearAssertions(key),
        ]);
        if (parsed.action === "publish" && !(await tx.companyAuthority(work)))
          throw ports.error(
            403,
            "LAYOUT_PUBLISH_FORBIDDEN",
            "Your organization authority changed.",
          );
      } else {
        const current = await tx.currentSession(session.session.id,user);
        if (!current)
          throw ports.error(401, "SESSION_EXPIRED", "Sign in again.");
        if (
          platform &&
          !(await tx.currentPlatform(platform.assignmentId,user,platform.role,ports.now(),ports.now()))
        )
          throw ports.error(
            403,
            "ASSIGNMENT_UNAVAILABLE",
            "Your platform role changed.",
          );
      }
      const prior = await tx.receipt(key);
      if (prior) {
        if (prior.request_hash !== hash)
          throw ports.error(
            409,
            "IDEMPOTENCY_CONFLICT",
            "Use the original command or a new key.",
          );
        const replay = JSON.parse(prior.result_json) as LayoutSnapshot;
        return {...replay,canPublish:!!canPublish,personal:replay.personal?{...replay.personal,widgets:replay.personal.widgets.filter(w=>ids.has(w.id))}:null,organization:replay.organization?{...replay.organization,widgets:replay.organization.widgets.filter(w=>ids.has(w.id))}:null};
      }
      const before = await read(tx),
        publish = parsed.action === "publish",
        current = publish ? before.organizationVersion : before.version;
      if (current !== parsed.expectedVersion)
        throw ports.error(
          409,
          "VERSION_CONFLICT",
          "The saved layout changed. Reload the latest layout before saving.",
        );
      const value =
        parsed.action === "reset" ? null : JSON.stringify(parsed.layout);
      await tx.save(company,publish?"organization":user,publish?"*":base,pageKey,value,current+1,ports.now());
      const after = await read(tx);
      await tx.saveReceipt(key,hash,JSON.stringify(after),user,company,base,pageKey,parsed.action,ports.now());
      return after;
    });
    return result;

};}
