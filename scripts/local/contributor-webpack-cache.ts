/** Aggregate developer diagnostics only; never log module paths or source. */
type ModuleStatistic={cached?:boolean;built?:boolean;modules?:ModuleStatistic[];children?:ModuleStatistic[]};
type CompilationStatistic={toJson(options:Record<string,boolean>):{modules?:ModuleStatistic[]}};
type Compiler={name?:string;hooks:{done:{tap(name:string,callback:(stats:CompilationStatistic)=>void):void}}};
export function contributorWebpackCachePlugin(){
 return {apply(compiler:Compiler){
  compiler.hooks.done.tap('ErpxContributorCache',stats=>{
   let cachedModules=0,builtModules=0;
   const visit=(modules:ModuleStatistic[])=>{for(const entry of modules){if(entry.cached===true)cachedModules++;if(entry.built===true)builtModules++;visit(entry.modules??[]);visit(entry.children??[]);}};
   visit(stats.toJson({all:false,modules:true,cachedModules:true,groupModulesByCacheStatus:false}).modules??[]);
   console.log(JSON.stringify({event:'ERPX_CONTRIBUTOR_WEBPACK_CACHE',compiler:compiler.name??'unknown',cachedModules,builtModules}));
  });
 }};
}
