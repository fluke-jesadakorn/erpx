import {expect,it} from "vitest";
import {readFileSync,readdirSync} from "node:fs";
import {join} from "node:path";
import {hasStaffNotificationResolver} from "../src/server/staff-notifications";

function files(root:string):string[]{return readdirSync(root,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?files(join(root,entry.name)):entry.name.endsWith(".ts")?[join(root,entry.name)]:[])}
it("has a current-access resolver for literal staff notification producer types",()=>{
  const missing=new Set<string>();
  for(const file of files(join(process.cwd(),"src/server"))){
    const source=readFileSync(file,"utf8");
    for(const match of source.matchAll(/INSERT\s+INTO\s+staff_notifications/gi)){
      const sql=source.slice(match.index,match.index+500);
      const type=sql.match(/(?:VALUES\s*\([^)]*|SELECT\s+[^;]*?)'([a-z_]+)'/i)?.[1];
      // `invoicing` occurs in a flow-routing status predicate before the bound record type.
      if(type&&type!=="invoicing"&&!hasStaffNotificationResolver(type))missing.add(`${file}: ${type}`);
    }
  }
  expect([...missing]).toEqual([]);
});
