// End-to-end integration test for XonVendingMachine.sol against a real Xode node.
//
// Two RPC surfaces are involved:
//  - polkadot.js against the Substrate WS endpoint (ws://127.0.0.1:9977), used for anything
//    native-Substrate: registering a local TrustBackedAsset to stand in for USDT, minting it,
//    and funding EVM signer accounts with native XON.
//  - ethers.js against the pallet_revive Ethereum JSON-RPC bridge (`eth-rpc`, http://127.0.0.1:8545),
//    used for everything EVM: deploying XonVendingMachine and calling buy/approve on it exactly
//    the way a real wallet/dApp would.
//
// Prerequisites (see zombienet/launch-ubuntu.sh):
//   ./bin2/zombienet-linux-x64 spawn zombienet-local.toml -p native --dir data   (brings up 9977)
//   ./bin2/eth-rpc --node-rpc-url ws://127.0.0.1:9977 --rpc-port 8545           (the EVM bridge)
//
// Usage:
//   cd ../contracts && forge build && cd ../xode-blockchain/client && node deployXonVendingMachine.js

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady, encodeAddress } from "@polkadot/util-crypto";
import { ethers } from "ethers";
import { readFileSync } from "node:fs";
import 'dotenv/config';

const XODE_WS = process.env.XODE_WS ?? "ws://127.0.0.1:9977";
const ETH_RPC = process.env.ETH_RPC ?? "http://127.0.0.1:8545";
const UNIT = 1_000_000_000_000n; // XON has 12 native decimals
const USDT_DECIMALS = 6;
const SS58_FORMAT = 42; // generic Substrate prefix used by this dev chain

const XonVendingMachineArtifact = JSON.parse(
  readFileSync(new URL("../../contracts/out/XonVendingMachine.sol/XonVendingMachine.json", import.meta.url)),
);
const IERC20_ABI = [
  "function balanceOf(address) view returns (uint256)",
  "function approve(address spender, uint256 amount) returns (bool)",
  "function allowance(address owner, address spender) view returns (uint256)",
];

function submit(api, tx, signer, label) {
  return new Promise((resolve, reject) => {
    tx.signAndSend(signer, (r) => {
      console.log(label, "status:", r.status.type);
      if (r.dispatchError) {
        const msg = r.dispatchError.isModule ? api.registry.findMetaError(r.dispatchError.asModule).name : r.dispatchError.toString();
        reject(new Error(label + " failed: " + msg));
        return;
      }
      if (r.status.isFinalized) resolve(r.events);
    }).catch(reject);
  });
}

// `AccountId32Mapper::to_fallback_account_id`: a plain secp256k1/Ethereum wallet's H160 address
// maps - with no prior registration needed - to the AccountId32 `[address bytes (20)] ++ 0xEE*12`.
// This is the account that actually needs native-XON balance for gas/value on the Substrate side.
function fallbackAccountId32(ethAddress) {
  const addrBytes = ethers.getBytes(ethAddress);
  const raw = new Uint8Array(32);
  raw.set(addrBytes, 0);
  raw.fill(0xee, 20);
  return raw;
}

function fallbackSs58(ethAddress) {
  return encodeAddress(fallbackAccountId32(ethAddress), SS58_FORMAT);
}

// `pallet_assets_precompiles::ERC20<Self, InlineIdConfig<0x0120>, TrustBackedAssetsInstance>`:
// address = [assetId as 4-byte BE][12 zero bytes][prefix 0x0120 as 2 bytes][2 zero bytes].
function trustBackedAssetPrecompileAddress(assetId) {
  const bytes = new Uint8Array(20);
  new DataView(bytes.buffer).setUint32(0, assetId, false);
  bytes[16] = 0x01;
  bytes[17] = 0x20;
  return ethers.getAddress(ethers.hexlify(bytes));
}

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");

  console.log(`Connecting to Xode ${XODE_WS} ...`);
  const xode = await ApiPromise.create({ provider: new WsProvider(XODE_WS) });

  const provider = new ethers.JsonRpcProvider(ETH_RPC);
  console.log(`Connected to eth-rpc ${ETH_RPC}, chainId:`, (await provider.getNetwork()).chainId);

  const deployer = ethers.Wallet.createRandom().connect(provider);
  const owner = ethers.Wallet.createRandom().connect(provider);
  const buyer = ethers.Wallet.createRandom().connect(provider);
  console.log("deployer:", deployer.address, "owner:", owner.address, "buyer:", buyer.address);

  console.log("\n=== 1. Fund each EVM signer's fallback AccountId32 with native XON ===");
  const fundAmount = 1_000n * UNIT;
  for (const [label, wallet] of [["deployer", deployer], ["owner", owner], ["buyer", buyer]]) {
    const ss58 = fallbackSs58(wallet.address);
    await submit(xode, xode.tx.balances.transferKeepAlive(ss58, fundAmount), alice, `fund ${label} (${ss58})`);
  }

  console.log("\n=== 2. Register (or reuse) a local TrustBackedAsset standing in for USDT ===");
  const assetId = Number(process.env.USDT_ASSET_ID ?? 1984);
  const assetDetails = await xode.query.assets.asset(assetId);
  if (assetDetails.isNone) {
    await submit(xode, xode.tx.assets.create(assetId, alice.address, 1), alice, "assets.create(USDT)");
    await submit(
      xode,
      xode.tx.assets.setMetadata(assetId, "Tether USD", "USDT", USDT_DECIMALS),
      alice,
      "assets.setMetadata(USDT)",
    );
  } else {
    console.log(`Asset ${assetId} already exists - reusing it.`);
  }

  const buyerSs58 = fallbackSs58(buyer.address);
  const usdtAmount = 1_000n * 10n ** BigInt(USDT_DECIMALS); // 1,000 test USDT
  await submit(xode, xode.tx.assets.mint(assetId, buyerSs58, usdtAmount), alice, "assets.mint(USDT -> buyer)");

  const usdtAddress = trustBackedAssetPrecompileAddress(assetId);
  console.log(`USDT asset id ${assetId} -> ERC20 precompile address ${usdtAddress}`);

  console.log("\n=== 3. Deploy XonVendingMachine (owner set at deploy, rate = 5 XON/USDT) ===");
  const rate = ethers.parseEther("5"); // Solidity sees native XON in 18-decimal "ether" units
  const factory = new ethers.ContractFactory(XonVendingMachineArtifact.abi, XonVendingMachineArtifact.bytecode.object, deployer);
  // No relayer for this demo - it only exercises the pull-based `buy()` path (approve + buy).
  // See demoRelayerBuy.js for the single-signature push/settlePurchase path.
  const vending = await factory.deploy(usdtAddress, owner.address, rate, ethers.ZeroAddress);
  await vending.waitForDeployment();
  console.log("XonVendingMachine deployed at:", await vending.getAddress());

  console.log("\n=== 4. Owner seeds the XON reserve (plain value transfer to the contract) ===");
  const seedTx = await owner.sendTransaction({ to: await vending.getAddress(), value: ethers.parseEther("100") });
  await seedTx.wait();
  console.log("Reserve balance:", ethers.formatEther(await provider.getBalance(await vending.getAddress())), "XON");

  console.log("\n=== 5. Buyer approves USDT, then buys XON ===");
  const usdt = new ethers.Contract(usdtAddress, IERC20_ABI, buyer);
  const buyUsdt = 10n * 10n ** BigInt(USDT_DECIMALS); // 10 USDT
  await (await usdt.approve(await vending.getAddress(), buyUsdt)).wait();

  const expectedXon = await vending.quote(buyUsdt);
  const buyerXonBefore = await provider.getBalance(buyer.address);

  const buyTx = await vending.connect(buyer).buy(buyUsdt);
  const receipt = await buyTx.wait();
  console.log("buy() tx:", receipt.hash, "status:", receipt.status);

  const buyerXonAfter = await provider.getBalance(buyer.address);
  const buyerUsdtAfter = await usdt.balanceOf(buyer.address);

  console.log("Expected XON out:", ethers.formatEther(expectedXon));
  console.log("Buyer XON delta (includes gas spent):", ethers.formatEther(buyerXonAfter - buyerXonBefore));
  console.log("Buyer USDT remaining:", buyerUsdtAfter.toString(), `(started with ${usdtAmount})`);

  if (buyerUsdtAfter !== usdtAmount - buyUsdt) {
    throw new Error("Buyer's USDT balance did not decrease by the expected amount.");
  }
  if (buyerXonAfter <= buyerXonBefore) {
    throw new Error("Buyer's native XON balance did not increase - buy() did not deliver.");
  }

  console.log("\nSUCCESS: XonVendingMachine buy() works end-to-end against a real Xode node via eth-rpc.");

  await xode.disconnect();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
