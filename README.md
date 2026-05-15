<img src="https://drive.google.com/uc?export=view&id=1i88iInwVlXMoX2L8P2kLwFzGNuFVRra5"></img>

<div align="center">

[![Twitter URL](https://img.shields.io/badge/Twitter-gray?logo=x)](https://x.com/XodeNet)
[![Facebook URL](https://img.shields.io/badge/Facebook-gray?logo=facebook)](https://web.facebook.com/xodenet)
[![Discord](https://img.shields.io/badge/Discord-gray?logo=discord)](https://discord.gg/V6DETUY7Cy)
<br>
[![Certik Security Code Audit Rating](https://img.shields.io/badge/Certik_Security_Code_Audit-AA-green.svg)](https://skynet.certik.com/projects/xode-blockchain)
[![CoinMarketCap](https://img.shields.io/badge/CoinMarketCap-Listing-purple.svg)](https://coinmarketcap.com/currencies/xode-blockchain/)
[![Kusama Parachain](https://img.shields.io/badge/Kusama_Parachain-3344-pink.svg)](https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpcnodea01.xode.net%2Fn7yoxCmcIrCF6VziCcDmYTwL8R03a%2Frpc#/explorer)
[![Polkadot Parachain](https://img.shields.io/badge/Polkadot_Parachain-3417-pink.svg)](https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-rpcnode.xode.net#/explorer)

> Xode is a blockchain platform with its own on-chain governance that aims to bring game development, decentralized finance, real-word assets and businesses to Web3 and Polkadot.
<br>

<a href="https://xterium.app/"><img style="width: 10%; height: 10%" src="https://drive.google.com/uc?export=view&id=1_6AzbP_ha5ZB8L_6VBZs8q5zUdEGpESP"></img></a>
<a href="https://novawallet.io/"><img style="width: 10%; height: 10%" src="https://drive.google.com/uc?export=view&id=1pJWJ6_n-XYmZreetrgSRnALkmt0BBpYe"></img></a>
<a href="https://talisman.xyz/"><img style="width: 10%; height: 10%" src="https://drive.google.com/uc?export=view&id=1EB4cD2qo9WhkWfFIvzO9YbA8KA5HLVHV"></img></a>
<a href="https://polkadot.js.org/extension/"><img style="width: 10%; height: 10%" src="https://drive.google.com/uc?export=view&id=1WpQuHdVVu1IyYtzbDirNWun4WUgm-_5c"></img></a>

> Web3 wallets that support Xode Blockchain include [Xterium](https://xterium.app/), [Nova Wallet](https://novawallet.io/), [Talisman](https://talisman.xyz/), and [PolkadotJS](https://polkadot.js.org/extension/), providing seamless integration and accessibility for users of the ecosystem.
<br>

</div>

#### Smart Contract Support
Xode Blockchain supports smart contracts (Wasm contracts) using the ink! framework, enabling developers to create and deploy secure and efficient decentralized applications. In the future, Xode Blockchain will also support EVM/Solidity contracts, expanding the ecosystem and allowing for greater interoperability with existing Ethereum-based applications.

#### Asset Hub 
Xode Blockchain has the pallet-assets installed, allowing for the creation, management, and transfer of custom assets on the network. This pallet enables the use of fungible tokens, providing flexibility for developers to build decentralized applications with token-based economies. In the future, Xode Blockchain will also introduce XCM Transport Methods (XCMP, HRMP, VMP) for cross-chain applications, enhancing interoperability and enabling seamless communication and asset transfers across different blockchains.

#### Proof-of-Stake
Xode Blockchain has built-in staking pallets to support fully decentralized nodes, enabling token holders to participate in the network's security and governance. These pallets facilitate the staking process, allowing candidates and stakers to secure the network while earning rewards for their contributions.  Join the [discussion](https://github.com/Xode-DAO/xode-blockchain/discussions/23).

#### Governance
Governance and treasury on Xode Blockchain are handled through the Xode Foundation, a Panama-based DAO, utilizing two collective pallets: the Treasury Council and the Technical Committee. These pallets facilitate decentralized decision-making and resource allocation, ensuring that the network's development and financial management are transparent, community-driven, and aligned with the ecosystem's long-term goals. Our goal is to expand the governance into OpenGov, further enhancing community involvement and decentralizing decision-making processes, paving the way for an even more inclusive and resilient governance structure.

#### [Block scanner](https://node.xode.net/)
Xode Blockchain integrates indexing and block scanning utilities through Subsquid, providing efficient data indexing and querying capabilities. This allows developers and users to easily access and analyze blockchain data, improving the overall user experience and enabling more advanced applications and analytics on the network.

## Ethereum Support

This guide walks you through setting up the Ethereum JSON-RPC adapter for pallet-revive, which allows Ethereum tooling (MetaMask, Remix, Web3.js) to interact with your Xode chain.

### Prerequisites

- Rust toolchain installed via `rustup`
- A running Substrate node with `pallet-revive` configured
- Rust version 1.91 or higher (required for eth-rpc dependencies)

### Installation Steps

#### 1. Install the latest Rust

```bash
# Update stable toolchain to the latest version
rustup update stable
rustup default stable

# Verify your Rust version is >= 1.91
rustc --version
```
#### 2. Install pallet-revive-eth-rpc

```bash
cargo install pallet-revive-eth-rpc --locked
```
#### 3. Start Xode RPC Node

```bash
# Navigate to your node directory
cd /path/to/xode-blockchain

# Run in development mode
./target/release/xode-node --dev --rpc-port 9944
```
#### 4. Start Xode Ethereum RPC Node

```bash
# Connect to custom node RPC port and listen on custom port
eth-rpc --node-rpc-url ws://127.0.0.1:9944 --rpc-port 8545
```
#### 5. Test Xode Ethereum RPC Endpoint

```bash
curl -X POST http://127.0.0.1:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc":"2.0",
    "method":"eth_getBlockByNumber",
    "params":["latest", false],
    "id":1
  }'
```
#### 6. Connecting MetaMask

1. Open MetaMask → Settings → Networks → Add Network
2. Configure:
   - **Network Name**: `Xode Local`
   - **RPC URL**: `http://127.0.0.1:8545`
   - **Chain ID**: Your parachain ID (e.g., `3417`)
   - **Currency Symbol**: `XON` (or your token symbol)


