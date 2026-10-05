/** Team-owned agent example: no browser cookie, company or role arguments. */
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";

const endpoint=process.env.ERPX_MCP_URL??"https://console-dev-erpx.jesadakorn.com/api/mcp/executive";
const secret=process.env.ERPX_MCP_TOKEN;
if(!secret)throw new Error("Set ERPX_MCP_TOKEN in your secret store or process environment.");
const client=new Client({name:"erpx-team-agent",version:"1.0.0"});
try {
  await client.connect(new StreamableHTTPClientTransport(new URL(endpoint),{authProvider:{token:async()=>secret}}));
  console.log(JSON.stringify(await client.listTools(),null,2));
  console.log(JSON.stringify(await client.callTool({name:"list_dashboards",arguments:{limit:20}}),null,2));
  console.log(JSON.stringify(await client.callTool({name:"list_metrics",arguments:{}}),null,2));
  if(process.env.ERPX_MCP_METRIC)console.log(JSON.stringify(await client.callTool({name:"query_metric",arguments:{metric:process.env.ERPX_MCP_METRIC,filters:{from:"2026-01-01",to:"2026-12-31",currency:"THB"},breakdown:"default",limit:100}}),null,2));
}finally{await client.close();}
