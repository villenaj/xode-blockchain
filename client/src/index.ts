import { createClient } from "polkadot-api"
import { withLogsRecorder } from "polkadot-api/logs-provider"
import { getWsProvider } from "polkadot-api/ws-provider/node"

const rpc = process.env.RPC;
const wsProvider = getWsProvider(rpc as string)

function test(): void {
  const provider = withLogsRecorder((line) => console.log(line), wsProvider)
  const client = createClient(provider)
  }
  
  test();