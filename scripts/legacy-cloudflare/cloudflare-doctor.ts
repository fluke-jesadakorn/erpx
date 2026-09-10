const required=["wrangler.jsonc","open-next.config.ts","migrations/control/0001_control.sql","migrations/holding-write/0001_holding.sql"];
let failed=false;for(const path of required){if(!(await Bun.file(path).exists())){console.error(`missing ${path}`);failed=true}}
const whoami=Bun.spawnSync(["bunx","wrangler","whoami"],{stdout:"pipe",stderr:"pipe"});if(whoami.exitCode===0)console.log(new TextDecoder().decode(whoami.stdout));else{console.error("Cloudflare CLI is not authenticated. Run `bunx wrangler login` or configure CLOUDFLARE_API_TOKEN.");failed=true}
if(failed)process.exit(1);console.log("ERPX Cloudflare development prerequisites are ready.");

export {};
