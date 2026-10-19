// Off-chain relayer for XonVendingMachine's push/single-signature buy path.
//
// A buyer who wants to pay with just ONE signature calls `usdt.transfer(vendingContract, amount)`
// directly - no `approve` step at all, since a plain transfer moves the caller's own balance with
// no allowance involved. But standard ERC20 `transfer` has no callback hook, so the contract can't
// react to that deposit within the same transaction. This script is what closes the loop: it polls
// for `Transfer` events into the vending contract and calls `settlePurchase` for each one it hasn't
// already settled.
//
// This relayer is trusted to report the right (buyer, amount, txHash) - but the contract itself
// never trusts it with the actual payout: `settlePurchase` only succeeds while the cumulative
// amount it has ever paid for stays within the contract's real, on-chain USDT balance (see
// `accountedUsdt` in XonVendingMachine.sol). A compromised relayer key can misdirect a real
// deposit; it cannot manufacture a payout with nothing behind it.
//
// Progress (the next block to scan from) is checkpointed to disk after every tick, keyed to this
// contract address, so a restart (crash, redeploy, server reboot) resumes from where it left off
// instead of silently skipping whatever arrived during the downtime. A deposit that fails to
// settle (a transient RPC error, the relayer running low on gas, etc.) blocks the checkpoint from
// advancing past it, so it's retried on the next tick rather than abandoned - safe to retry
// because `settlePurchase` is idempotent (the contract's `settledDeposits` check, mirrored here).
//
// Usage:
//   VENDING_ADDRESS=0x... RELAYER_PRIVATE_KEY=0x... node relayXonVendingMachine.js

import { ethers } from "ethers";
import { readFileSync, writeFileSync, renameSync, existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import 'dotenv/config';

const ETH_RPC = process.env.ETH_RPC ?? "http://127.0.0.1:8545";
const VENDING_ADDRESS = process.env.VENDING_ADDRESS;
const RELAYER_PRIVATE_KEY = process.env.RELAYER_PRIVATE_KEY;
const POLL_INTERVAL_MS = Number(process.env.POLL_INTERVAL_MS ?? 5000);
// How many confirmed blocks to wait before treating a transfer as final, to stay correct across
// reorgs. Local/dev chains barely reorg (0-1 is fine); use a few more on a real network.
const CONFIRMATIONS = Number(process.env.CONFIRMATIONS ?? 1);

if (!VENDING_ADDRESS || !RELAYER_PRIVATE_KEY) {
  console.error("Set VENDING_ADDRESS and RELAYER_PRIVATE_KEY (the address returned by setRelayer/constructor).");
  process.exit(1);
}

// Only used on a genuinely first run (no checkpoint file yet). Defaults to "start from whatever
// block is current" - fine for a relayer started before any deposits happen, but if you're
// bootstrapping against a contract that might already have unsettled deposits, set this to its
// deployment block explicitly so the first run doesn't skip them.
const START_BLOCK = process.env.START_BLOCK ? Number(process.env.START_BLOCK) : null;
const CHECKPOINT_FILE = process.env.CHECKPOINT_FILE ?? `.relayer-checkpoint-${VENDING_ADDRESS.toLowerCase()}.json`;

const XonVendingMachineArtifact = JSON.parse(
  readFileSync(new URL("../../contracts/out/XonVendingMachine.sol/XonVendingMachine.json", import.meta.url)),
);
const TRANSFER_TOPIC = ethers.id("Transfer(address,address,uint256)");

function loadCheckpoint() {
  if (!existsSync(CHECKPOINT_FILE)) return null;
  const parsed = JSON.parse(readFileSync(CHECKPOINT_FILE, "utf8"));
  if (parsed.vendingAddress?.toLowerCase() !== VENDING_ADDRESS.toLowerCase()) {
    throw new Error(
      `${CHECKPOINT_FILE} was written for ${parsed.vendingAddress}, not ${VENDING_ADDRESS}. ` +
      `Set CHECKPOINT_FILE to a different path for this contract, or remove the stale file if that's expected.`,
    );
  }
  return parsed.fromBlock;
}

// Atomic (write-then-rename) so a crash mid-write can't leave a corrupt/partial checkpoint file.
function saveCheckpoint(fromBlock) {
  const dir = dirname(CHECKPOINT_FILE);
  if (dir !== "." && !existsSync(dir)) mkdirSync(dir, { recursive: true });
  const tmpFile = `${CHECKPOINT_FILE}.tmp`;
  writeFileSync(tmpFile, JSON.stringify({ vendingAddress: VENDING_ADDRESS, fromBlock }, null, 2));
  renameSync(tmpFile, CHECKPOINT_FILE);
}

async function main() {
  const provider = new ethers.JsonRpcProvider(ETH_RPC);
  const relayerWallet = new ethers.Wallet(RELAYER_PRIVATE_KEY, provider);
  const vending = new ethers.Contract(VENDING_ADDRESS, XonVendingMachineArtifact.abi, relayerWallet);

  const usdtAddress = await vending.usdt();
  const onChainRelayer = await vending.relayer();
  if (onChainRelayer.toLowerCase() !== relayerWallet.address.toLowerCase()) {
    throw new Error(
      `RELAYER_PRIVATE_KEY (${relayerWallet.address}) does not match the contract's relayer (${onChainRelayer}). ` +
      `Call setRelayer first, or use the right key.`,
    );
  }

  console.log(`Watching USDT (${usdtAddress}) transfers into ${VENDING_ADDRESS}`);
  console.log(`Relayer: ${relayerWallet.address}, polling every ${POLL_INTERVAL_MS}ms, ${CONFIRMATIONS} confirmation(s).`);

  const checkpointed = loadCheckpoint();
  let fromBlock;
  if (checkpointed !== null) {
    fromBlock = checkpointed;
    console.log(`Resuming from checkpoint (${CHECKPOINT_FILE}): block ${fromBlock}.`);
  } else if (START_BLOCK !== null) {
    fromBlock = START_BLOCK;
    console.log(`No checkpoint found. Starting from START_BLOCK=${fromBlock}.`);
  } else {
    fromBlock = (await provider.getBlockNumber()) + 1;
    console.warn(
      `No checkpoint and no START_BLOCK set - starting from the current block (${fromBlock}). ` +
      `Any deposit already sitting in the contract before this moment will NOT be picked up.`,
    );
  }

  async function tick() {
    const latest = await provider.getBlockNumber();
    const toBlock = latest - CONFIRMATIONS;
    if (toBlock < fromBlock) return;

    // This bridge's eth_getLogs doesn't accept `null`/`[]` as a topic wildcard (unlike standard
    // Ethereum nodes - confirmed directly against it, both are rejected or match nothing), so
    // filter only by topic0 (Transfer) here and match the recipient client-side below.
    const logs = await provider.getLogs({
      address: usdtAddress,
      topics: [TRANSFER_TOPIC],
      fromBlock,
      toBlock,
    });

    const paddedVending = ethers.zeroPadValue(VENDING_ADDRESS, 32).toLowerCase();
    let stalledAtBlock = null;

    for (const log of logs) {
      if (log.topics[2].toLowerCase() !== paddedVending) continue;

      const buyer = ethers.getAddress(ethers.dataSlice(log.topics[1], 12));
      const amount = ethers.toBigInt(log.data);
      const depositTxHash = log.transactionHash;

      const alreadySettled = await vending.settledDeposits(depositTxHash);
      if (alreadySettled) {
        console.log(`Skipping ${depositTxHash} - already settled.`);
        continue;
      }

      console.log(`New deposit: ${buyer} sent ${amount.toString()} USDT (tx ${depositTxHash}). Settling...`);
      try {
        const tx = await vending.settlePurchase(buyer, amount, depositTxHash);
        const receipt = await tx.wait();
        console.log(`Settled ${depositTxHash} in block ${receipt.blockNumber} (settle tx ${receipt.hash}).`);
      } catch (err) {
        // Don't let the checkpoint advance past this deposit - retry it (and anything after it)
        // next tick instead of silently abandoning it. Safe to redo: settlePurchase is idempotent.
        console.error(`Failed to settle ${depositTxHash}, will retry next tick:`, err.shortMessage ?? err.message);
        stalledAtBlock = log.blockNumber;
        break;
      }
    }

    fromBlock = stalledAtBlock ?? toBlock + 1;
    saveCheckpoint(fromBlock);
  }

  // eslint-disable-next-line no-constant-condition
  while (true) {
    await tick().catch((err) => console.error("Poll tick failed:", err.shortMessage ?? err.message));
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
