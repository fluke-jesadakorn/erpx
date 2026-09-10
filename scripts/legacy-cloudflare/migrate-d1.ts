const remote=process.argv.includes("--remote");const envIndex=process.argv.indexOf("--env");const environment=envIndex>=0?process.argv[envIndex+1]:undefined;
const databases=["erpx-control-write","erpx-control-read","erpx-h001-write","erpx-h001-read","erpx-h002-write","erpx-h002-read"];
for(const database of databases){const args=["wrangler","d1","migrations","apply",database,remote?"--remote":"--local"];if(environment)args.push("--env",environment);const processResult=Bun.spawnSync(["bunx",...args],{stdout:"inherit",stderr:"inherit"});if(processResult.exitCode!==0)process.exit(processResult.exitCode)}

export {};
