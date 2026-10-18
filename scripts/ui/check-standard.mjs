import {existsSync,readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
import ts from 'typescript';
const root=resolve(import.meta.dirname,'../..');
const manifest=JSON.parse(readFileSync(resolve(root,'docs/ux-ui/standard-coverage.json'),'utf8'));
const controls=new Set(['Button','TextInput','TextArea','Select','Field','Card','TableFrame','StatusBadge','PageHeader','Dialog','FormDrawer','FormDisclosure','ListFilters','IconButton','ActionLink','ActionAnchor','FeatureTabBar']);
const violations=(manifest.boundaryConsumers??[]).map(x=>`${x.owner}: ${x.component} needs a legacy boundary adapter`);
// Only the receipt validator may derive accepted status; generated/manual strings are not proof.
for(const source of manifest.sources)source.verification=source.scope?.startsWith('deferred')||source.scope==='frozen-specialized'?'excluded':'pending';
for(const route of manifest.routes)route.verification=route.scope?.startsWith('deferred')||route.scope==='frozen-specialized'?'excluded':'pending';
let acceptance={sources:new Map(),routes:new Map(),transitions:new Map()};
if(existsSync(resolve(root,'docs/ux-ui/acceptance-receipts.json'))) {
 try {const receipts=await import('./acceptance.mjs');acceptance=await receipts.loadUiAcceptance(root,manifest);receipts.applyUiAcceptance(manifest,acceptance);}
 catch(error){violations.push(`UI acceptance refused: ${error instanceof Error?error.message:'invalid receipt'}`);}
}
const frozen=JSON.parse(readFileSync(resolve(root,'scripts/ui/frozen-boundaries.json'),'utf8'));
// Retired entrypoints retain their original Git evidence. Their explicit successors
// are drift-protected separately; this is not specialized browser acceptance.
const retirementPath=resolve(root,'scripts/ui/frozen-retirements.json');
const retirements=existsSync(retirementPath)?JSON.parse(readFileSync(retirementPath,'utf8')):[];
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const seenRetirements=new Set();
for(const retirement of retirements) {
 if(seenRetirements.has(retirement.source)||!frozen.some(file=>file.source===retirement.source))violations.push(`${retirement.source}: invalid frozen retirement`);
 seenRetirements.add(retirement.source);
}
for(const file of frozen) {
 const path=resolve(root,file.source),retirement=retirements.find(entry=>entry.source===file.source);
 if(existsSync(path)) {
  if(retirement)violations.push(`${file.source}: retired entrypoint reappeared`);
  if(hash(readFileSync(path))!==file.fingerprint&&acceptance.transitions.get(file.source)?.current.binding?.sha256!==hash(readFileSync(path)))violations.push(`${file.source}: frozen boundary changed (${file.reason})`);
  continue;
 }
 if(!retirement) {violations.push(`${file.source}: frozen boundary missing without reviewed retirement`);continue;}
 if(!/^[a-f0-9]{40}$/.test(retirement.originalCommit??'')||!retirement.reviewReason||retirement.acceptance!=='pending'||!Array.isArray(retirement.successors)||!retirement.successors.length) {
  violations.push(`${file.source}: invalid frozen retirement evidence`);continue;
 }
 const original=spawnSync('git',['show',`${retirement.originalCommit}:${file.source}`],{cwd:root});
 if(original.status!==0||!original.stdout||hash(original.stdout)!==file.fingerprint)violations.push(`${file.source}: original frozen evidence unavailable or changed`);
 for(const successor of retirement.successors) {
  const successorPath=resolve(root,successor.source);
  if(!successor.source.startsWith('src/')||!existsSync(successorPath)||hash(readFileSync(successorPath))!==successor.fingerprint&&acceptance.transitions.get(file.source)?.successors.get(successor.source)!==hash(readFileSync(successorPath)))violations.push(`${file.source}: retired successor missing or changed: ${successor.source}`);
 }
}
for(const source of manifest.sources) {
 if(source.scope!=='standardize'||source.presentation!=='shared-opt-in-present')continue;
 const text=readFileSync(resolve(root,source.source),'utf8');
 if(createHash('sha256').update(text).digest('hex')!==source.fingerprint)violations.push(`${source.source}: inventory is stale; regenerate coverage`);
 const ast=ts.createSourceFile(source.source,text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const importedControls=new Map();
 for(const node of ast.statements) if(ts.isImportDeclaration(node)&&ts.isStringLiteral(node.moduleSpecifier)&&/^@\/components\/(?:ui\/|shared\/features\/feature-page)/.test(node.moduleSpecifier.text)) {
  const bindings=node.importClause?.namedBindings;
  if(bindings&&ts.isNamedImports(bindings))for(const specifier of bindings.elements) {
   const original=(specifier.propertyName??specifier.name).text;if(controls.has(original))importedControls.set(specifier.name.text,original);
  }
 }
 const visit=node=>{
  if(ts.isJsxOpeningElement(node)||ts.isJsxSelfClosingElement(node)) {
   const tag=node.tagName.getText(ast),attrs=node.attributes.properties;
   if(tag==='table')violations.push(`${source.source}:${ast.getLineAndCharacterOfPosition(node.getStart(ast)).line+1} native table bypasses shared presentation`);
   const appearance=attrs.find(a=>ts.isJsxAttribute(a)&&a.name.getText(ast)==='appearance');
   if(importedControls.has(tag)&&!appearance) violations.push(`${source.source}:${ast.getLineAndCharacterOfPosition(node.getStart(ast)).line+1} ${tag} lacks explicit appearance`);
   for(const attr of attrs)if(ts.isJsxAttribute(attr)&&attr.name.getText(ast)==='className'&&attr.initializer&&ts.isStringLiteral(attr.initializer)&&/(?:^|\s)(card|business-form|section-head|form-actions)(?:\s|$)/.test(attr.initializer.text))violations.push(`${source.source}:${ast.getLineAndCharacterOfPosition(attr.getStart(ast)).line+1} legacy presentation class`);
  }
  ts.forEachChild(node,visit);
 };visit(ast);
}
console.log(violations.length?violations.join('\n'):'Migrated caller presentation guard passed.');
if(violations.length)process.exitCode=1;
if(process.argv.includes('--complete')) {
 const pendingRetirements=retirements.filter(entry=>!acceptance.transitions.has(entry.source));
 if(pendingRetirements.length){console.error(`Completion blocked: ${pendingRetirements.length} retired boundaries require specialized browser acceptance.`);process.exitCode=1;}
 const pending=manifest.routes.filter(x=>x.verification==='pending'||x.verification==='excluded'&&x.scope!=='deferred-public-marketing');
 const pendingSources=manifest.sources.filter(x=>x.verification==='pending'||x.verification==='excluded'&&x.scope!=='deferred-public-marketing'&&manifest.routes.some(route=>route.source===x.source||(route.dependencies??[]).includes(x.source)));
 if(pendingSources.length){console.error(`Completion blocked: ${pendingSources.length} component/page sources lack current state acceptance.`);process.exitCode=1;}
 if(pending.length){ console.error(`Completion blocked: ${pending.length} routes lack current browser acceptance.`);process.exitCode=1; }
}
