# Kills each service by the specific port it's bound to (fuser), not by process name - a plain
# `pkill eth-rpc` can't tell the Xode and AssetHub eth-rpc instances apart since they're the same
# binary, and would kill both at once even if you only wanted to restart one.
ANVIL_PORT=8545
XODE_ETH_RPC_PORT=8546
ASSETHUB_ETH_RPC_PORT=8547

fuser -k ${ANVIL_PORT}/tcp 2>/dev/null
fuser -k ${XODE_ETH_RPC_PORT}/tcp 2>/dev/null
fuser -k ${ASSETHUB_ETH_RPC_PORT}/tcp 2>/dev/null

# zombienet spawns a whole process tree (relay validators + both collators) under one orchestrator,
# not a single port - kill it by process name instead.
pkill -f zombienet-linux

