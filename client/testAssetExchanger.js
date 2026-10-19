// Drives the exact scenario covered by runtime/src/configs/xcm_config/tests.rs against a real,
// running Xode node - proving GatedAssetExchanger (XcmConfig::AssetExchanger) behaves correctly
// with real weights/fees/executor code, not the Rust-mock genesis used by the unit tests.
//
// Unlike a true cross-chain test, this doesn't need a second parachain, HRMP channels, or asset
// registry governance: it submits the XCM program to Xode itself via `polkadotXcm.execute`, which
// is exactly what a wallet's "self-serve swap" flow would do, and exercises the same
// `ExchangeAsset` codepath a message arriving from another chain would.
//
// Usage:
//   WS_ENDPOINT=ws://127.0.0.1:9988 node client/testAssetExchanger.js
// (9988 = "charlie" in zombienet/zombienet.toml; use 9977 for "xoxe-collator-one" if you launched
// zombienet-local.toml instead.)

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import 'dotenv/config';

const WS_ENDPOINT = process.env.WS_ENDPOINT ?? "ws://127.0.0.1:9988";
const LOCAL_ASSET_ID = 42;
const UNIT = 1_000_000_000_000n; // 1 XON, 12 decimals

function assetKind(kind) {
  // Matches `runtime::configs::AssetKind` - `{ Native: null }` or `{ Local: assetId }`.
  return kind === "native" ? { Native: null } : { Local: LOCAL_ASSET_ID };
}

// Waits for *finalization*, not just inclusion: solo collators in this zombienet topology reorg
// shallowly every 30-60s, which can retract a block containing our extrinsic. The pool then
// reports a terminal-but-non-fatal status (Dropped/Invalid/Usurped/Retracted) instead of ever
// finalizing - unhandled, that just hangs the promise forever, so it's treated as retryable here.
function submitOnce(tx, signer, label) {
  return new Promise((resolve, reject) => {
    tx.signAndSend(signer, ({ status, dispatchError, events }) => {
      if (dispatchError) {
        const msg = dispatchError.isModule
          ? JSON.stringify(dispatchError.asModule.toHuman())
          : dispatchError.toString();
        reject(new Error(`${label} failed: ${msg}`));
        return;
      }
      if (status.isFinalized) {
        console.log(`${label}: finalized in block ${status.asFinalized}`);
        resolve(events);
        return;
      }
      if (status.isDropped || status.isInvalid || status.isUsurped || status.isRetracted) {
        reject(new Error(`RETRYABLE:${label} hit pool status '${status.type}' (likely a reorg) - resubmitting`));
      }
    }).catch(reject);
  });
}

async function submit(tx, signer, label, attempts = 5) {
  for (let i = 1; i <= attempts; i++) {
    try {
      return await submitOnce(tx, signer, label);
    } catch (e) {
      if (!e.message.startsWith("RETRYABLE:") || i === attempts) throw e;
      console.log(`  attempt ${i}/${attempts}: ${e.message} - retrying ...`);
      await new Promise((r) => setTimeout(r, 6000));
    }
  }
}

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");   // pool creator, technical committee member
  const bob = keyring.addFromUri("//Bob");       // technical committee member (approver)
  const charlie = keyring.addFromUri("//Charlie"); // technical committee member (approver)
  const dave = keyring.addFromUri("//Dave");     // beneficiary of the swap

  console.log(`Connecting to ${WS_ENDPOINT} ...`);
  const api = await ApiPromise.create({ provider: new WsProvider(WS_ENDPOINT) });

  console.log("1) Creating local asset and minting a supply to Alice ...");
  await submit(
    api.tx.assets.create(LOCAL_ASSET_ID, alice.address, 1),
    alice,
    "assets.create",
  );
  await submit(
    api.tx.assets.mint(LOCAL_ASSET_ID, alice.address, 1_000n * UNIT),
    alice,
    "assets.mint",
  );

  console.log("2) Opening a Native/Local(42) pool (pending governance approval) ...");
  await submit(
    api.tx.assetWaiting.createPool(assetKind("native"), assetKind("local")),
    alice,
    "assetWaiting.createPool",
  );

  console.log("3) Approving the pool (2 of 3 technical committee members) ...");
  await submit(
    api.tx.assetWaiting.approvePool(assetKind("native"), assetKind("local")),
    bob,
    "assetWaiting.approvePool (Bob)",
  );
  await submit(
    api.tx.assetWaiting.approvePool(assetKind("native"), assetKind("local")),
    charlie,
    "assetWaiting.approvePool (Charlie)",
  );

  console.log("4) Seeding liquidity now that the pool is Active ...");
  await submit(
    api.tx.assetConversion.addLiquidity(
      assetKind("native"), assetKind("local"),
      100n * UNIT, 100n * UNIT, 1, 1, alice.address,
    ),
    alice,
    "assetConversion.addLiquidity",
  );

  console.log("5) Submitting the swap-and-forward XCM program via polkadotXcm.execute ...");
  const swapIn = 10n * UNIT;
  const fee = 1n * UNIT;
  const message = {
    V5: [
      { WithdrawAsset: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: swapIn + fee } }] },
      { BuyExecution: { fees: { id: { parents: 0, interior: "Here" }, fun: { Fungible: fee } }, weightLimit: "Unlimited" } },
      {
        ExchangeAsset: {
          give: { Definite: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: swapIn } }] },
          want: [{
            id: { parents: 0, interior: { X2: [{ PalletInstance: 50 }, { GeneralIndex: LOCAL_ASSET_ID }] } },
            fun: { Fungible: 1 },
          }],
          maximal: false,
        },
      },
      {
        DepositAsset: {
          assets: { Wild: "All" },
          beneficiary: {
            parents: 0,
            interior: { X1: [{ AccountId32: { network: null, id: dave.publicKey } }] },
          },
        },
      },
    ],
  };
  const weightLimit = { refTime: 20_000_000_000n, proofSize: 2_000_000n };
  await submit(
    api.tx.polkadotXcm.execute(message, weightLimit),
    alice,
    "polkadotXcm.execute",
  );

  console.log("6) Checking Dave's balance of the local asset ...");
  const balance = await api.query.assets.account(LOCAL_ASSET_ID, dave.address);
  console.log("Dave's Local(42) balance:", balance.isSome ? balance.unwrap().balance.toString() : "0 (none)");

  if (balance.isNone || balance.unwrap().balance.eqn(0)) {
    throw new Error("Swap did not deliver funds to the beneficiary - check node logs (xcm=trace).");
  }
  console.log("Swap-and-forward succeeded.");

  await api.disconnect();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
