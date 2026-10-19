ETH_RPC=http://127.0.0.1:8545 \
VENDING_ADDRESS=0x54b961D139e377070CA6c3f3E3D12eC645e32b0d \
RELAYER_PRIVATE_KEY=c177890f0ab788d3fdc65cbecabcafe9883653b9db33463a1dcd7fabbce1ed9c \
nohup node ../client/relayXonVendingMachine.js > relayer.log 2>&1 &
