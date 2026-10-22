import { readdirSync, readFileSync, writeFileSync, existsSync, mkdirSync, statSync } from 'node:fs';
import { resolve, dirname, relative } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { scopeFor } from './standard-scope.mjs';
import { appPageContext, appPageBoundaries } from './route-topology.mjs';
const root = resolve(import.meta.dirname, '../..');
const walk = dir => readdirSync(dir, {withFileTypes:true}).flatMap(x => x.isDirectory() ? ['node_modules','.next','.build','.cache','.turbo'].includes(x.name)?[]:walk(resolve(dir,x.name)) : x.isFile()?[resolve(dir,x.name)]:[]);
const rel = x => relative(root,x).replaceAll('\\','/');
const files = ['src','apps','packages'].filter(name=>existsSync(resolve(root,name))).flatMap(name=>walk(resolve(root,name))).filter(x=>/\.(tsx?|css)$/.test(x)).sort((a,b)=>{
 const left=rel(a),right=rel(b);return left<right?-1:left>right?1:0;
});
const hash = text => createHash('sha256').update(text).digest('hex');
const sourceHashes=new Map(files.map(file=>[rel(file),hash(readFileSync(file))]));
const resolveImport = (owner, name) => {
  const workspace=/^@erpx\/([^/]+)\/(.+)$/.exec(name);
  if (!workspace && !name.startsWith('.') && !name.startsWith('@/')) return null;
  const base = workspace ? resolve(root,'packages',workspace[1],'src',workspace[2]) : name.startsWith('@/') ? resolve(root,'src',name.slice(2)) : resolve(dirname(owner),name);
  return ['', '.tsx', '.ts', '/index.tsx', '/index.ts', '.css'].map(s=>base+s).find(candidate=>existsSync(candidate)&&statSync(candidate).isFile()) ?? null;
};
// These explicit type edges are erased under this workspace's TypeScript
// configuration (verbatimModuleSyntax is not enabled). Keep conservative value
// edges, including empty named clauses, and literal dynamic imports unchanged.
const runtimeModuleSpecifier = node => {
 if (ts.isImportDeclaration(node)) {
  const clause=node.importClause, bindings=clause?.namedBindings;
  if (clause?.phaseModifier===ts.SyntaxKind.TypeKeyword || (!clause?.name && bindings && ts.isNamedImports(bindings) && bindings.elements.length>0 && bindings.elements.every(element=>element.isTypeOnly))) return null;
  return node.moduleSpecifier;
 }
 if (ts.isExportDeclaration(node)) {
  const bindings=node.exportClause;
  if (node.isTypeOnly || (bindings && ts.isNamedExports(bindings) && bindings.elements.length>0 && bindings.elements.every(element=>element.isTypeOnly))) return null;
  return node.moduleSpecifier;
 }
 return ts.isCallExpression(node) && node.expression.kind===ts.SyntaxKind.ImportKeyword ? node.arguments[0] : null;
};
const sources = files.filter(x=>x.endsWith('.tsx')).map(file=> {
 const text = readFileSync(file,'utf8');
 const ast = ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const imports = new Set(), components = new Set(), surfaces = [], queries = new Set(), accessChecks = new Set();
 const visit = node => {
   if(ts.isCallExpression(node)&&/^(?:can|hasPermission|hasAnyPermission|require\w+|assert\w+|allowedTransitions)$/.test(node.expression.getText(ast)))accessChecks.add(node.getText(ast).slice(0,300));
   const specifier=runtimeModuleSpecifier(node);
   if (specifier && ts.isStringLiteral(specifier)) {
     const target = resolveImport(file,specifier.text); if(target) imports.add(rel(target));
   }
   if (ts.isJsxOpeningElement(node)||ts.isJsxSelfClosingElement(node)) {
     const tag = node.tagName.getText(ast); components.add(tag);
     if(/Dialog|Drawer|Disclosure|Menu|Popover|Picker|Tabs|TabNav|Table|RecordList|Loading|Empty|Alert|Notice/.test(tag)||tag==='details'||node.attributes.properties.some(a=>ts.isJsxAttribute(a)&&a.name.getText(ast)==='aria-expanded')) {
       const props=Object.fromEntries(node.attributes.properties.filter(a=>ts.isJsxAttribute(a)&&['title','summary','label','aria-label','presentation','open','disabled','readOnly'].includes(a.name.getText(ast))).map(a=>[a.name.getText(ast),a.initializer?.getText(ast)??true]));
       let parent=node.parent;const conditions=[];
       while(parent&&!ts.isFunctionDeclaration(parent)&&!ts.isArrowFunction(parent)) {
         if(ts.isConditionalExpression(parent))conditions.push(parent.condition.getText(ast).slice(0,240));
         if(ts.isBinaryExpression(parent)&&parent.operatorToken.kind===ts.SyntaxKind.AmpersandAmpersandToken)conditions.push(parent.left.getText(ast).slice(0,240));
         parent=parent.parent;
       }
       surfaces.push({component:tag,line:ast.getLineAndCharacterOfPosition(node.getStart(ast)).line+1,props,conditions,trigger:props.summary??props['aria-label']??props.title??'caller/data dependent; inspect owner',verification:'pending'});
     }
   }
   if (ts.isStringLiteral(node)) {
      for(const match of node.text.matchAll(/[?&]([a-zA-Z][a-zA-Z0-9_-]*)=([^&#\s]+)/g)) queries.add(`${match[1]}=${match[2]}`);
   }
   ts.forEachChild(node,visit);
 }; visit(ast);
 const scope=scopeFor(rel(file));
 return {source:rel(file),scope,fingerprint:hash(text),imports:[...imports].sort(),components:[...components].sort(),queryViews:[...queries].sort(),accessChecks:[...accessChecks].sort(),targetPatterns:[
   ...(components.has('Table')||components.has('StandardTable')?['register-table']:[]),
   ...(components.has('RecordList')||components.has('RecordListItem')?['narrative-list']:[]),
   ...(components.has('FormLayout')||components.has('FormGrid')?['native-form']:[]),
   ...([...components].some(tag=>/Drawer|Disclosure/.test(tag))?['form-drawer']:[]),
   ...([...components].some(tag=>/LocalTabs|TabNav|TabsList|FeatureTabBar/.test(tag))?['tabs']:[]),
 ],surfaces,styles:[...imports].filter(x=>x.endsWith('.css')),presentation:/appearance=(?:"standard"|\{appearance\})|ui-(?:panel|stack|page-content|record-item|form-grid|filter-bar)/.test(text)?'shared-opt-in-present':'review-required',verification:scope.startsWith('deferred')||scope==='frozen-specialized'?'excluded':'pending'};
});
// Include TS registries/re-exports in reachability without counting server helpers as components.
const graph = new Map(sources.map(x=>[x.source,x]));
for (const file of files.filter(x=>x.endsWith('.ts'))) {
 const ast=ts.createSourceFile(file,readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const imports=new Set();
 const visit=node=> {
  const specifier=runtimeModuleSpecifier(node);
  if(specifier&&ts.isStringLiteral(specifier)) { const target=resolveImport(file,specifier.text); if(target)imports.add(rel(target)); }
  ts.forEachChild(node,visit);
 };visit(ast);graph.set(rel(file),{imports:[...imports]});
}
for(const file of files.filter(x=>x.endsWith('.css'))) {
 const imports=[];
 for(const match of readFileSync(file,'utf8').matchAll(/@import\s+["']([^"']+)["']/g)) {
  const target=resolveImport(file,match[1]);if(target)imports.push(rel(target));
 }
 graph.set(rel(file),{imports});
}
const map = new Map(sources.map(x=>[x.source,x]));
const closure = (path,seen=new Set()) => { if(seen.has(path))return seen; seen.add(path); for(const edge of graph.get(path)?.imports??[]) if(graph.has(edge))closure(edge,seen); return seen; };
const availableSources=new Set(files.map(rel));
const hasWorkspaceRoutes=sources.some(source=>/^apps\/[^/]+\/src\/app\//.test(source.source));
const routes = sources.filter(x=>appPageContext(x.source)!==null && (!hasWorkspaceRoutes || x.source.startsWith('apps/'))).map(x=> {
 const text=readFileSync(resolve(root,x.source),'utf8');
 const reachable = closure(x.source);
 // Framework convention files are loader-tree edges, not source imports.
 const boundaries=appPageBoundaries(x.source,availableSources);
 for(const file of boundaries)closure(file,reachable);
 const dependencies=[...reachable].filter(file=>map.has(file)).sort();
 const styles=[...new Set([...reachable].filter(file=>file.endsWith('.css')))].sort();
 const fingerprints=[...reachable,...styles].filter(file=>existsSync(resolve(root,file))).map(file=>`${file}:${sourceHashes.get(file)??hash(readFileSync(resolve(root,file)))}`);
 const fingerprint=hash(fingerprints.sort().join('\n'));
 const authorization=[...text.matchAll(/\b(?:require|assert|can|authorize)[A-Z]\w*\([^;\n]*/g)].map(x=>x[0]);
 const {route,routeContext}=appPageContext(x.source);
 return {route,routeContext,source:x.source,scope:x.scope,fingerprint,boundaries,closure:[...reachable].sort(),authorization,kind:/\bredirect\(/.test(text)&& !/[<][A-Za-z]/.test(text)?'redirect':'screen',roleScope:'verify every observed authorization/permission branch with an authorized session',accessChecks:[...new Set(dependencies.flatMap(d=>map.get(d)?.accessChecks??[]))],targetPatterns:[...new Set(dependencies.flatMap(d=>map.get(d)?.targetPatterns??[]))],stateChecklist:['loaded','loading','empty','error','validation','disabled/read-only','permission/data conditional','pagination/search/reset','portalled popup/draft focus','print if document'],queryViews:[...new Set(dependencies.flatMap(d=>map.get(d)?.queryViews??[]))],dependencies,styles,evidence:[],verification:x.verification};
});
const boundaryConsumers=[];
for(const owner of sources.filter(x=>x.verification==='excluded')) {
 for(const dependency of closure(owner.source)) {
  const child=map.get(dependency);
  if(dependency!==owner.source&&child?.scope==='standardize')boundaryConsumers.push({owner:owner.source,component:dependency,reason:'An ordinary migrated component is reachable from an excluded owner; add an explicit legacy adapter or document a narrower boundary.'});
 }
}
const styleSources=files.filter(file=>file.endsWith('.css')).map(file=>({source:rel(file),fingerprint:hash(readFileSync(file)),scope:scopeFor(rel(file))}));
const closurePathSet=new Set();
for(const route of routes)for(const path of route.closure)closurePathSet.add(path);
const closurePaths=[...closurePathSet].sort(),closurePathIds=new Map(closurePaths.map((path,index)=>[path,index]));
const sourceEdges=closurePaths.map(file=>[...(graph.get(file)?.imports??[])].filter(file=>closurePathIds.has(file)).map(file=>closurePathIds.get(file)).sort((a,b)=>a-b));
for(const route of routes){route.entryIds=[route.source,...route.boundaries].map(path=>closurePathIds.get(path)).sort((a,b)=>a-b);delete route.closure;delete route.dependencies;}

const output={schemaVersion:5,boundaryConsumers,closurePaths,sourceEdges,styleSources,notes:['Source inventory is not browser acceptance.','Pending states include conditional permissions, data, pagination, portals and print.','Excluded subtrees keep legacy defaults; shared owners require caller-specific evidence.'],summary:{routes:routes.length,components:sources.filter(x=>/^(?:src\/components\/|packages\/[^/]+\/src\/components\/)/.test(x.source)).length,pendingRoutes:routes.filter(x=>x.verification==='pending').length,pendingSources:sources.filter(x=>x.verification==='pending').length},routes,sources};
if(existsSync(resolve(root,'docs/ux-ui/acceptance-receipts.json'))) {
 const receipts=await import('./acceptance.mjs');
 receipts.applyUiAcceptance(output,await receipts.loadUiAcceptance(root,output));
 output.summary.pendingRoutes=output.routes.filter(row=>row.verification==='pending').length;
 output.summary.pendingSources=output.sources.filter(row=>row.verification==='pending').length;
 output.summary.acceptedRoutes=output.routes.filter(row=>row.verification==='accepted').length;
 output.summary.acceptedSources=output.sources.filter(row=>row.verification==='accepted').length;
}
mkdirSync(resolve(root,'docs/ux-ui'),{recursive:true});
writeFileSync(resolve(root,'docs/ux-ui/standard-coverage.json'),JSON.stringify(output,null,2)+'\n');
console.log(JSON.stringify(output.summary));
