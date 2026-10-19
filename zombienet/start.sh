#!/usr/bin/env bash
# Launches the full local stack in one go, in dependency order:
#   1. zombienet (relay chain + AssetHub collator + Xode collator)
#   2. eth-rpc for Xode and eth-rpc for AssetHub (one instance per chain - see process-kill.sh)
#   3. anvil (local Ethereum stand-in)
#
# Each step waits until the previous one is actually serving before moving on, since eth-rpc fails
# immediately if its --node-rpc-url isn't reachable yet. Everything runs in the background; stop it
# all with ./process-kill.sh.
#
# Usage: ./launch-all.sh [--clean] [--no-anvil]
#   --clean     wipe zombienet data/ and old logs first (keeps anvil-state.json)
#   --no-anvil  skip anvil (only needed when testing the Ethereum leg)
#
# Env overrides: WAIT_TIMEOUT (seconds per readiness wait, default 600)

set -euo pipefail
cd "$(dirname "$0")"

XODE_NODE_PORT=9977
ASSETHUB_NODE_PORT=9955
ANVIL_PORT=8545
XODE_ETH_RPC_PORT=8546
ASSETHUB_ETH_RPC_PORT=8547
WAIT_TIMEOUT=${WAIT_TIMEOUT:-600}

CLEAN=0
ANVIL=1
for arg in "$@"; do
  case "$arg" in
    --clean) CLEAN=1 ;;
    --no-anvil) ANVIL=0 ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

for bin in ./bin2/zombienet-linux-x64 ./bin2/eth-rpc ./bin2/polkadot ./bin2/polkadot-parachain ../target/release/xode-node; do
  [[ -x "$bin" ]] || { echo "Missing executable: $bin" >&2; exit 1; }
done
if (( ANVIL )) && ! command -v anvil >/dev/null; then
  echo "anvil not found in PATH (install Foundry, or pass --no-anvil)" >&2; exit 1
fi

port_in_use() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }
for port in $XODE_NODE_PORT $ASSETHUB_NODE_PORT $XODE_ETH_RPC_PORT $ASSETHUB_ETH_RPC_PORT $( (( ANVIL )) && echo $ANVIL_PORT ); do
  if port_in_use "$port"; then
    echo "Port $port is already in use - run ./process-kill.sh first" >&2; exit 1
  fi
done

rpc() { # rpc <port> <method>
  curl -s -m 3 -H "Content-Type: application/json" \
    -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"$2\",\"params\":[]}" "http://127.0.0.1:$1" 2>/dev/null || true
}

# wait_until <description> <pid> <log> <check-command...>
# Polls the check every 3s; bails out early (with the log tail) if the process it depends on dies.
wait_until() {
  local desc=$1 pid=$2 log=$3; shift 3
  local start=$SECONDS
  printf '  waiting for %s' "$desc"
  until "$@"; do
    if ! kill -0 "$pid" 2>/dev/null; then
      printf '\n  process exited - last lines of %s:\n' "$log"; tail -n 20 "$log"; exit 1
    fi
    if (( SECONDS - start > WAIT_TIMEOUT )); then
      printf '\n  timed out after %ss - see %s\n' "$WAIT_TIMEOUT" "$log"; exit 1
    fi
    printf '.'; sleep 3
  done
  printf ' ok (%ss)\n' "$(( SECONDS - start ))"
}

# Block number > 0 means the parachain is onboarded and producing blocks, not just that RPC is up.
producing_blocks() {
  local num
  num=$(rpc "$1" chain_getHeader | grep -o '"number":"0x[0-9a-fA-F]*"' | grep -o '0x[0-9a-fA-F]*' || true)
  [[ -n "$num" ]] && (( num > 0 ))
}
eth_rpc_ready() { rpc "$1" eth_blockNumber | grep -q '"result"'; }

if (( CLEAN )); then
  echo "Cleaning data/ and logs"
  rm -rf data zombienet.log eth-rpc-xode.log eth-rpc-assethub.log anvil.log
fi

echo "[1/3] zombienet"
yes | DEBUG=zombie* nohup ./bin2/zombienet-linux-x64 spawn zombienet.toml -p native --dir data > zombienet.log 2>&1 &
ZOMBIE_PID=$!
wait_until "Xode producing blocks (:$XODE_NODE_PORT)" $ZOMBIE_PID zombienet.log producing_blocks $XODE_NODE_PORT
wait_until "AssetHub producing blocks (:$ASSETHUB_NODE_PORT)" $ZOMBIE_PID zombienet.log producing_blocks $ASSETHUB_NODE_PORT

echo "[2/3] eth-rpc"
nohup ./bin2/eth-rpc --node-rpc-url ws://127.0.0.1:$XODE_NODE_PORT --rpc-port $XODE_ETH_RPC_PORT --prometheus-port 9616 > eth-rpc-xode.log 2>&1 &
XODE_ETH_PID=$!
nohup ./bin2/eth-rpc --node-rpc-url ws://127.0.0.1:$ASSETHUB_NODE_PORT --rpc-port $ASSETHUB_ETH_RPC_PORT --prometheus-port 9617 > eth-rpc-assethub.log 2>&1 &
ASSETHUB_ETH_PID=$!
wait_until "Xode eth-rpc (:$XODE_ETH_RPC_PORT)" $XODE_ETH_PID eth-rpc-xode.log eth_rpc_ready $XODE_ETH_RPC_PORT
wait_until "AssetHub eth-rpc (:$ASSETHUB_ETH_RPC_PORT)" $ASSETHUB_ETH_PID eth-rpc-assethub.log eth_rpc_ready $ASSETHUB_ETH_RPC_PORT

if (( ANVIL )); then
  echo "[3/3] anvil"
  nohup anvil --port $ANVIL_PORT --block-time 6 --state ./anvil-state.json --state-interval 10 > anvil.log 2>&1 &
  ANVIL_PID=$!
  wait_until "anvil (:$ANVIL_PORT)" $ANVIL_PID anvil.log eth_rpc_ready $ANVIL_PORT
else
  echo "[3/3] anvil skipped"
fi

cat <<EOF

All services running (stop with ./process-kill.sh):
  Xode substrate RPC       ws://127.0.0.1:$XODE_NODE_PORT
  AssetHub substrate RPC   ws://127.0.0.1:$ASSETHUB_NODE_PORT
  Xode eth-rpc             http://127.0.0.1:$XODE_ETH_RPC_PORT
  AssetHub eth-rpc         http://127.0.0.1:$ASSETHUB_ETH_RPC_PORT
EOF
(( ANVIL )) && echo "  anvil (Ethereum)         http://127.0.0.1:$ANVIL_PORT"
echo "Logs: zombienet.log eth-rpc-xode.log eth-rpc-assethub.log$( (( ANVIL )) && echo ' anvil.log')"
