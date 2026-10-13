import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
import ts from 'typescript';
const root=resolve(import.meta.dirname,'../..');
const manifest=JSON.parse(readFileSync(resolve(root,'docs/ux-ui/standard-coverage.json'),'utf8'));
const controls=new Set(['Button','TextInput','TextArea','Select','Field','Card','TableFrame','StatusBadge','PageHeader','Dialog','FormDrawer','FormDisclosure','ListFilters','IconButton','ActionLink','ActionAnchor','FeatureTabBar']);
const violations=(manifest.boundaryConsumers??[]).map(x=>`${x.owner}: ${x.component} needs a legacy boundary adapter`);
const frozen=JSON.parse(readFileSync(resolve(root,'scripts/ui/frozen-boundaries.json'),'utf8'));
for(const file of frozen) {
 const current=createHash('sha256').update(readFileSync(resolve(root,file.source))).digest('hex');
 if(current!==file.fingerprint)violations.push(`${file.source}: frozen boundary changed (${file.reason})`);
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
 const pending=manifest.routes.filter(x=>x.verification==='pending');
 const pendingSources=manifest.sources.filter(x=>x.verification==='pending');
 if(pendingSources.length){console.error(`Completion blocked: ${pendingSources.length} component/page sources lack current state acceptance.`);process.exitCode=1;}
 if(pending.length){ console.error(`Completion blocked: ${pending.length} routes lack current browser acceptance.`);process.exitCode=1; }
}
