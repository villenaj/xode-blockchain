// Demonstrates XonVendingMachine's single-signature buy path: the buyer sends ONE transaction
// (a plain `usdt.transfer` to the contract, no `approve` at all), and a separately-running
// relayer (client/relayXonVendingMachine.js) notices the deposit and calls `settlePurchase` to
// pay out XON - all without the buyer signing anything a second time.
//
// This script deploys a fresh XonVendingMachine, starts the relayer as a child process pointed at
// it, has the buyer do their one-signature transfer, and waits for the relayer to settle it.
//
// Usage:
//   cd ../contracts && forge build && cd ../xode-blockchain/client && node demoRelayerBuy.js

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady, encodeAddress } from "@polkadot/util-crypto";
import { ethers } from "ethers";
import { readFileSync } from "node:fs";
import { spawn } from "node:child_process";
import 'dotenv/config';

const XODE_WS = process.env.XODE_WS ?? "ws://127.0.0.1:9977";
const ETH_RPC = process.env.ETH_RPC ?? "http://127.0.0.1:8545";
const UNIT = 1_000_000_000_000n;
const USDT_DECIMALS = 6;
const SS58_FORMAT = 42;

const XonVendingMachineArtifact = JSON.parse(
  readFileSync(new URL("../../contracts/out/XonVendingMachine.sol/XonVendingMachine.json", import.meta.url)),
);
const IERC20_ABI = [
  "function transfer(address to, uint256 amount) returns (bool)",
  "function balanceOf(address) view returns (uint256)",
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

function fallbackAccountId32(ethAddress) {
  const raw = new Uint8Array(32);
  raw.set(ethers.getBytes(ethAddress), 0);
  raw.fill(0xee, 20);
  return raw;
}

function fallbackSs58(ethAddress) {
  return encodeAddress(fallbackAccountId32(ethAddress), SS58_FORMAT);
}

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

  const xode = await ApiPromise.create({ provider: new WsProvider(XODE_WS) });
  const provider = new ethers.JsonRpcProvider(ETH_RPC);

  const deployer = ethers.Wallet.createRandom().connect(provider);
  const owner = ethers.Wallet.createRandom().connect(provider);
  const relayer = ethers.Wallet.createRandom().connect(provider);
  const buyer = ethers.Wallet.createRandom().connect(provider);
  console.log("deployer:", deployer.address);
  console.log("owner:   ", owner.address);
  console.log("relayer: ", relayer.address);
  console.log("buyer:   ", buyer.address);

  console.log("\n=== 1. Fund EVM signers' fallback AccountId32s with native XON ===");
  for (const [label, wallet, amount] of [
    ["deployer", deployer, 1_000n * UNIT],
    ["owner", owner, 1_000n * UNIT],
    ["relayer", relayer, 1_000n * UNIT], // pays gas for settlePurchase calls
    ["buyer", buyer, 1_000n * UNIT],
  ]) {
    const ss58 = fallbackSs58(wallet.address);
    await submit(xode, xode.tx.balances.transferKeepAlive(ss58, amount), alice, `fund ${label} (${ss58})`);
  }

  console.log("\n=== 2. Reuse (or create) the local TrustBackedAsset standing in for USDT ===");
  const assetId = Number(process.env.USDT_ASSET_ID ?? 1984);
  const assetDetails = await xode.query.assets.asset(assetId);
  if (assetDetails.isNone) {
    await submit(xode, xode.tx.assets.create(assetId, alice.address, 1), alice, "assets.create(USDT)");
    await submit(xode, xode.tx.assets.setMetadata(assetId, "Tether USD", "USDT", USDT_DECIMALS), alice, "assets.setMetadata(USDT)");
  }
  const buyerSs58 = fallbackSs58(buyer.address);
  const usdtAmount = 1_000n * 10n ** BigInt(USDT_DECIMALS);
  await submit(xode, xode.tx.assets.mint(assetId, buyerSs58, usdtAmount), alice, "assets.mint(USDT -> buyer)");
  const usdtAddress = trustBackedAssetPrecompileAddress(assetId);
  console.log(`USDT asset id ${assetId} -> ERC20 precompile address ${usdtAddress}`);

  console.log("\n=== 3. Deploy XonVendingMachine with relayer set ===");
  const rate = ethers.parseEther("5");
  const factory = new ethers.ContractFactory(XonVendingMachineArtifact.abi, XonVendingMachineArtifact.bytecode.object, deployer);
  const vending = await factory.deploy(usdtAddress, owner.address, rate, relayer.address);
  await vending.waitForDeployment();
  const vendingAddress = await vending.getAddress();
  console.log("XonVendingMachine deployed at:", vendingAddress);

  console.log("\n=== 4. Owner seeds the XON reserve ===");
  await (await owner.sendTransaction({ to: vendingAddress, value: ethers.parseEther("100") })).wait();

  console.log("\n=== 5. Start the relayer watcher as a background process ===");
  const relayerProc = spawn(
    process.execPath,
    [new URL("./relayXonVendingMachine.js", import.meta.url).pathname],
    {
      env: {
        ...process.env,
        ETH_RPC,
        VENDING_ADDRESS: vendingAddress,
        RELAYER_PRIVATE_KEY: relayer.privateKey,
        POLL_INTERVAL_MS: "2000",
        CONFIRMATIONS: "0",
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  // The relayer computes its starting block (`fromBlock`) only after it finishes connecting and
  // reading the contract's usdt()/relayer() - if the buyer's deposit landed before that happens,
  // it would fall outside the relayer's forward-only window and never be picked up. Wait for its
  // "Watching ..." readiness line before sending anything, so there's no race.
  let resolveReady;
  const relayerReady = new Promise((resolve) => { resolveReady = resolve; });
  relayerProc.stdout.on("data", (d) => {
    process.stdout.write(`[relayer] ${d}`);
    if (d.toString().includes("Watching USDT")) resolveReady();
  });
  relayerProc.stderr.on("data", (d) => process.stderr.write(`[relayer] ${d}`));

  try {
    await relayerReady;
    console.log("\n=== 6. Buyer sends ONE transaction: a plain usdt.transfer, no approve ===");
    const usdt = new ethers.Contract(usdtAddress, IERC20_ABI, buyer);
    const buyUsdt = 10n * 10n ** BigInt(USDT_DECIMALS);
    const buyerXonBefore = await provider.getBalance(buyer.address);

    const transferTx = await usdt.transfer(vendingAddress, buyUsdt);
    const transferReceipt = await transferTx.wait();
    console.log("Buyer's single signed transaction:", transferReceipt.hash);

    console.log("\n=== 7. Waiting for the relayer to notice and settle it ===");
    const deadline = Date.now() + 60_000;
    let settled = false;
    while (Date.now() < deadline) {
      settled = await vending.settledDeposits(transferReceipt.hash);
      if (settled) break;
      await new Promise((r) => setTimeout(r, 1000));
    }
    if (!settled) throw new Error("Relayer did not settle the deposit within 60s.");

    const buyerXonAfter = await provider.getBalance(buyer.address);
    console.log("Buyer XON delta (buyer never called buy() or approve()):", ethers.formatEther(buyerXonAfter - buyerXonBefore));

    if (buyerXonAfter <= buyerXonBefore) {
      throw new Error("Buyer's XON balance did not increase.");
    }

    console.log("\nSUCCESS: single-signature buy via relayer works end-to-end.");
  } finally {
    relayerProc.kill();
  }

  await xode.disconnect();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
