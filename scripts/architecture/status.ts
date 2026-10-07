import fs from "node:fs";
import path from "node:path";
import {serviceOwnership} from "../../src/server/modules/ownership";

const root=path.resolve(import.meta.dirname,"../..");
function files(directory:string):string[]{if(!fs.existsSync(directory))return [];return fs.readdirSync(directory,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?files(path.join(directory,entry.name)):[path.join(directory,entry.name)]).filter(file=>/\.(ts|tsx)$/.test(file));}

/** Boundary compliance and semantic migration completion are separate evidence. */
export function architectureStatus(projectRoot=root,inventory=serviceOwnership){
 const owners=inventory.owners.map(owner=>{
  const directory=path.join(projectRoot,"src/server/modules",owner.id),legacy=files(path.join(directory,"adapters/legacy")).map(file=>path.relative(projectRoot,file).replaceAll("\\","/"));
  const incompleteCapabilities=(owner.capabilities??[]).filter(capability=>capability.migration!=="clean").map(capability=>({id:capability.id,migration:capability.migration??"unset"}));
  return {owner:owner.id,kind:owner.kind,migration:owner.migration,cleanCapabilities:(owner.capabilities??[]).filter(capability=>capability.migration==="clean").map(capability=>capability.id),incompleteCapabilities,legacyImplementations:legacy,domainFiles:files(path.join(directory,"domain")).length,applicationFiles:files(path.join(directory,"application")).length};
 });
 return {complete:owners.every(owner=>owner.migration==="clean"&&owner.incompleteCapabilities.length===0&&owner.legacyImplementations.length===0),owners,legacyImplementations:owners.reduce((sum,owner)=>sum+owner.legacyImplementations.length,0)};
}

if(import.meta.main){const status=architectureStatus();console.log(JSON.stringify(status,null,2));if(process.argv.includes("--require-clean")&&!status.complete)process.exitCode=1;}
