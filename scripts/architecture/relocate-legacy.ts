import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import {serviceOwnership} from "../../src/server/modules/ownership";
import {resolveSource} from "./boundaries";

const root=path.resolve(import.meta.dirname,"../..");
type Relocation={source:string;target:string;owner:string;hazards:string[];exports:{values:string[];types:string[]};implementation:string};

/** Physical ownership only. This never marks a legacy capability as clean. */
export function relocationPlan(projectRoot=root,options:{includeKernels?:boolean;owners?:typeof serviceOwnership.owners}={}):Relocation[]{
 const kernels=new Set(["commerce","billing","records","workflow","files"]);
 const candidates=(options.owners??serviceOwnership.owners).filter(owner=>owner.kind==="business"||(options.includeKernels&&kernels.has(owner.id))).flatMap(owner=>owner.source.filter(source=>source.startsWith("src/server/")&&!source.startsWith("src/server/modules/")&&!source.includes("*")).map(source=>({source,owner:owner.id}))).filter(entry=>fs.existsSync(path.join(projectRoot,entry.source)));
 const program=ts.createProgram(candidates.map(entry=>path.join(projectRoot,entry.source)),{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,baseUrl:projectRoot,paths:{"@/*":["./src/*"]},skipLibCheck:true,noEmit:true});
 const checker=program.getTypeChecker();
 return candidates.flatMap(({source,owner})=>{
  const text=fs.readFileSync(path.join(projectRoot,source),"utf8"),parsed=program.getSourceFile(path.join(projectRoot,source))!;
  // Existing compatibility modules already have physical ownership.
  if(parsed.statements.every(node=>ts.isExportDeclaration(node)||(ts.isImportDeclaration(node)&&!node.importClause&&ts.isStringLiteral(node.moduleSpecifier)&&node.moduleSpecifier.text==="server-only")))return [];
  const symbol=checker.getSymbolAtLocation(parsed),values:string[]=[],types:string[]=[];
  if(symbol)for(const exported of checker.getExportsOfModule(symbol)){const resolved=exported.flags&ts.SymbolFlags.Alias?checker.getAliasedSymbol(exported):exported;(resolved.flags&ts.SymbolFlags.Value?values:types).push(exported.name);}
  const target=`src/server/modules/${owner}/adapters/legacy/${source.slice("src/server/".length)}`,hazards:string[]=[],edits:{start:number;end:number;text:string}[]=[];
  const rewrite=(literal:ts.StringLiteral)=>{if(!literal.text.startsWith("."))return;const resolved=resolveSource(projectRoot,source,literal.text);if(!resolved?.startsWith("src/")){hazards.push(`Non-src relative dependency: ${literal.text}`);return;}edits.push({start:literal.getStart(parsed),end:literal.getEnd(),text:JSON.stringify("@/"+resolved.slice(4).replace(/\.(ts|tsx|mts)$/,""))});};
  const visit=(node:ts.Node)=>{
   if((ts.isImportDeclaration(node)||ts.isExportDeclaration(node))&&node.moduleSpecifier&&ts.isStringLiteral(node.moduleSpecifier))rewrite(node.moduleSpecifier);
   if(ts.isImportTypeNode(node)&&ts.isLiteralTypeNode(node.argument)&&ts.isStringLiteral(node.argument.literal))rewrite(node.argument.literal);
   if(ts.isCallExpression(node)&&(node.expression.kind===ts.SyntaxKind.ImportKeyword||(ts.isIdentifier(node.expression)&&node.expression.text==="require"))){if(node.arguments[0]&&ts.isStringLiteral(node.arguments[0]))rewrite(node.arguments[0]);else hazards.push("Nonliteral dynamic module resolution");}
   if(ts.isMetaProperty(node)&&node.keywordToken===ts.SyntaxKind.ImportKeyword)hazards.push("import.meta depends on original module location");
   if(ts.isIdentifier(node)&&["__dirname","__filename"].includes(node.text))hazards.push(`${node.text} depends on original module location`);
   ts.forEachChild(node,visit);
  };visit(parsed);
  if(!values.length&&!types.length)hazards.push("No exported public surface; side-effect-only compatibility needs explicit review");
  if(fs.existsSync(path.join(projectRoot,target)))hazards.push("Destination already exists");
  let implementation=text;for(const edit of edits.sort((a,b)=>b.start-a.start))implementation=implementation.slice(0,edit.start)+edit.text+implementation.slice(edit.end);
  return [{source,target,owner,hazards:[...new Set(hazards)],exports:{values:values.sort(),types:types.sort()},implementation}];
 }).sort((a,b)=>a.source.localeCompare(b.source));
}

function compatibility(entry:Relocation){const specifier="@/"+entry.target.slice(4,-3);return `/** Compatibility entry point; legacy implementation belongs to ${entry.owner}. */\n`+(entry.exports.values.length?`export {${entry.exports.values.join(",")}} from ${JSON.stringify(specifier)};\n`:"")+(entry.exports.types.length?`export type {${entry.exports.types.join(",")}} from ${JSON.stringify(specifier)};\n`:"");}

export function applyRelocation(plan:Relocation[],projectRoot=root){
 if(plan.some(entry=>entry.hazards.length))throw new Error("Relocation requires review of every reported hazard; run without --write for the plan.");
 for(const entry of plan)if(fs.existsSync(path.join(projectRoot,entry.target)))throw new Error(`Relocation destination already exists: ${entry.target}`);
 // Compute and validate the entire move before writing any file.
 for(const entry of plan){fs.mkdirSync(path.dirname(path.join(projectRoot,entry.target)),{recursive:true});fs.writeFileSync(path.join(projectRoot,entry.target),entry.implementation);fs.writeFileSync(path.join(projectRoot,entry.source),compatibility(entry));}
}

if(import.meta.main){
 const plan=relocationPlan(root,{includeKernels:process.argv.includes("--include-kernels")});
 if(process.argv.includes("--write")){
  applyRelocation(plan);
  console.log(`Relocated ${plan.length} legacy implementations. Migration statuses remain unchanged.`);
 }else console.log(JSON.stringify({count:plan.length,owners:[...new Set(plan.map(entry=>entry.owner))].sort(),candidates:plan.map(entry=>({source:entry.source,target:entry.target,owner:entry.owner,hazards:entry.hazards,exports:entry.exports}))},null,2));
}
