// True cross-chain "swap along the way": AssetHub sends an asset to Xode via
// `polkadotXcm.transferAssetsUsingTypeAndThen`, with a `custom_xcm_on_dest` tail that runs
// `ExchangeAsset` (backed by GatedAssetExchanger) then deposits the swapped-out native token to
// a beneficiary - one call from AssetHub, no second transaction on Xode.
//
// This script is fully self-sufficient: it provisions everything needed from scratch (HRMP
// channels, both assets on both chains, the accounts XCM deposits into, and a governance-approved
// AMM pool with liquidity on Xode), so it works against a freshly spawned zombienet. Every step
// checks existing state first, so it's also safe to re-run against a network it already set up.
//
// Two assets travel together because `transferAssetsUsingTypeAndThen` always constructs a paid
// `BuyExecution` on arrival, and Xode's `DynamicWeightTrader` only accepts three specific fee
// assets (native XON, DOT, or AssetHub's "USDT" - asset id 1984) - an arbitrary swapped asset
// can't pay its own way in. So: asset 42 is what gets swapped, asset 1984 pays the XCM fee.
//
// Usage:
//   ASSETHUB_WS=ws://127.0.0.1:9955 XODE_WS=ws://127.0.0.1:9977 RELAY_WS=ws://127.0.0.1:9911 \
//     node client/testCrossChainSwap.js

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import 'dotenv/config';

const ASSETHUB_WS = process.env.ASSETHUB_WS ?? "ws://127.0.0.1:9955";
const XODE_WS = process.env.XODE_WS ?? "ws://127.0.0.1:9977";
const RELAY_WS = process.env.RELAY_WS ?? "ws://127.0.0.1:9911";
const XODE_PARA_ID = 3417;
const ASSETHUB_PARA_ID = 1000;
const ASSET_ID = 42; // the asset being swapped
const USDT_ID = 1984; // pays the XCM execution fee on arrival at Xode
const UNIT = 1_000_000_000_000n; // 12 decimals, matches XON and this test's asset 42
const UNIT_USDT = 1_000_000n; // 6 decimals, matches real AssetHub USDT
const SWAP_AMOUNT = 10n * UNIT;
const FEE_AMOUNT = 10n * UNIT_USDT;
// Hardcoded fee-collector account `DynamicWeightTrader` deposits fees into
// (runtime/src/configs/xcm_config/weight_trader.rs) - must exist (hold some native balance)
// before it can receive a non-sufficient asset like our local USDT mirror.
const FEE_COLLECTOR = "5Fqc6tG16FcR4QLwknqs5sb9Pc6pqsNwtwcPh7fNKCaP4DJT";

// Waits for *finalization*, not just inclusion: this zombienet topology's solo collators can
// reorg shallowly, which retracts a block containing our extrinsic. The pool then reports one of
// several terminal-but-non-fatal statuses (Dropped/Invalid/Usurped/Retracted) instead of ever
// finalizing - unhandled, that hangs the promise forever, so it's treated as retryable here.
function submitOnce(api, tx, signer, label) {
  return new Promise((resolve, reject) => {
    tx.signAndSend(signer, ({ status, dispatchError, events }) => {
      if (dispatchError) {
        const msg = dispatchError.isModule
          ? api.registry.findMetaError(dispatchError.asModule).name
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
      // Ready / Broadcast / InBlock: keep waiting.
    }).catch(reject);
  });
}

async function submit(api, tx, signer, label, attempts = 5) {
  for (let i = 1; i <= attempts; i++) {
    try {
      return await submitOnce(api, tx, signer, label);
    } catch (e) {
      if (!e.message.startsWith("RETRYABLE:") || i === attempts) throw e;
      console.log(`  attempt ${i}/${attempts}: ${e.message} - retrying ...`);
      await new Promise((r) => setTimeout(r, 6000));
    }
  }
}

// A fresh rococo-local relay chain-spec sets `asyncBackingParams` to {0,0} (async backing
// effectively disabled), which is incompatible with how modern collators (this repo's pinned
// polkadot-parachain/xode-node) build blocks - it causes persistent shallow reorgs on both
// parachains, silently dropping any extrinsic that happens to land in a losing fork. Every
// transaction in this script depends on the network actually being able to finalize things, so
// this must run before anything else.
async function ensureAsyncBacking(relay, alice) {
  const cfg = await relay.query.configuration.activeConfig();
  if (cfg.asyncBackingParams.maxCandidateDepth.toNumber() > 0) {
    console.log("Async backing already enabled.");
    return;
  }
  console.log("Async backing disabled on this chain-spec - enabling via relay sudo ...");
  await submit(
    relay,
    relay.tx.sudo.sudo(relay.tx.configuration.setAsyncBackingParams({ maxCandidateDepth: 3, allowedAncestryLen: 2 })),
    alice,
    "configuration.setAsyncBackingParams",
  );
  console.log("Waiting for the new params to activate (takes effect at the next session) ...");
  for (let i = 0; i < 40; i++) {
    const cfg2 = await relay.query.configuration.activeConfig();
    if (cfg2.asyncBackingParams.maxCandidateDepth.toNumber() > 0) {
      console.log("Async backing active.");
      return;
    }
    await new Promise((r) => setTimeout(r, 6000));
  }
  throw new Error("Async backing params did not activate in time");
}

async function ensureHrmpChannels(relay, alice) {
  const [a, b] = await Promise.all([
    relay.query.hrmp.hrmpChannels([ASSETHUB_PARA_ID, XODE_PARA_ID]),
    relay.query.hrmp.hrmpChannels([XODE_PARA_ID, ASSETHUB_PARA_ID]),
  ]);
  if (a.isSome && b.isSome) {
    console.log("HRMP channels already open both ways.");
    return;
  }
  console.log("Opening HRMP channels via relay sudo ...");
  const cfg = await relay.query.configuration.activeConfig();
  const cap = cfg.hrmpChannelMaxCapacity;
  const size = cfg.hrmpChannelMaxMessageSize;
  if (!a.isSome) {
    await submit(relay, relay.tx.sudo.sudo(relay.tx.hrmp.forceOpenHrmpChannel(ASSETHUB_PARA_ID, XODE_PARA_ID, cap, size)), alice, "forceOpenHrmpChannel(1000,3417)");
  }
  if (!b.isSome) {
    await submit(relay, relay.tx.sudo.sudo(relay.tx.hrmp.forceOpenHrmpChannel(XODE_PARA_ID, ASSETHUB_PARA_ID, cap, size)), alice, "forceOpenHrmpChannel(3417,1000)");
  }
  console.log("Waiting for channels to activate (takes effect at the next session) ...");
  for (let i = 0; i < 40; i++) {
    const [a2, b2] = await Promise.all([
      relay.query.hrmp.hrmpChannels([ASSETHUB_PARA_ID, XODE_PARA_ID]),
      relay.query.hrmp.hrmpChannels([XODE_PARA_ID, ASSETHUB_PARA_ID]),
    ]);
    if (a2.isSome && b2.isSome) {
      console.log("Both channels active.");
      return;
    }
    await new Promise((r) => setTimeout(r, 6000));
  }
  throw new Error("HRMP channels did not become active in time");
}

// Ensures `assetId` exists as a self-service trust-backed asset on `api`, owned/issued by
// `signer`, with at least `mintAmount` minted to `signer`.
async function ensureAssetSeeded(api, assetId, signer, mintAmount, label) {
  const existing = await api.query.assets.asset(assetId);
  if (existing.isNone) {
    await submit(api, api.tx.assets.create(assetId, signer.address, 1), signer, `${label}.assets.create(${assetId})`);
  }
  const balance = await api.query.assets.account(assetId, signer.address);
  const have = balance.isSome ? BigInt(balance.unwrap().balance.toString()) : 0n;
  if (have < mintAmount) {
    await submit(api, api.tx.assets.mint(assetId, signer.address, mintAmount), signer, `${label}.assets.mint(${assetId})`);
  }
}

// Ensures `account` exists (has a nonzero native balance) on `api`, funding it from `signer` if
// not - required before it can receive deposits of a non-sufficient asset.
async function ensureAccountFunded(api, account, signer, amount, label) {
  const state = await api.query.system.account(account);
  if (state.providers.toNumber() > 0) {
    console.log(`${label}: already exists.`);
    return;
  }
  await submit(api, api.tx.balances.transferKeepAlive(account, amount), signer, `${label}.fund`);
}

async function ensurePoolReady(xode, alice, bob, charlie) {
  const native = { Native: null };
  const localAsset = { Local: ASSET_ID };

  const details = await xode.query.assetWaiting.poolDetailsById([native, localAsset]);
  if (details.isNone) {
    await submit(xode, xode.tx.assetWaiting.createPool(native, localAsset), alice, "xode.assetWaiting.createPool");
  }
  const afterCreate = await xode.query.assetWaiting.poolDetailsById([native, localAsset]);
  const status = afterCreate.unwrap().status.toString();
  if (status === "Pending") {
    for (const approver of [bob, charlie]) {
      try {
        await submit(xode, xode.tx.assetWaiting.approvePool(native, localAsset), approver, `xode.assetWaiting.approvePool(${approver.address})`);
      } catch (e) {
        if (!/AlreadyApproved/.test(e.message)) throw e;
      }
    }
  }
  await submit(xode, xode.tx.assetConversion.addLiquidity(native, localAsset, 100n * UNIT, 100n * UNIT, 1, 1, alice.address), alice, "xode.assetConversion.addLiquidity");
}

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");
  const bob = keyring.addFromUri("//Bob");
  const charlie = keyring.addFromUri("//Charlie");
  const dave = keyring.addFromUri("//Dave");

  console.log(`Connecting to relay ${RELAY_WS}, AssetHub ${ASSETHUB_WS}, Xode ${XODE_WS} ...`);
  const [relay, assetHub, xode] = await Promise.all([
    ApiPromise.create({ provider: new WsProvider(RELAY_WS) }),
    ApiPromise.create({ provider: new WsProvider(ASSETHUB_WS) }),
    ApiPromise.create({ provider: new WsProvider(XODE_WS) }),
  ]);

  console.log("=== Setup ===");
  await ensureAsyncBacking(relay, alice);
  await ensureHrmpChannels(relay, alice);

  console.log("Seeding asset 42 and USDT (1984) on AssetHub ...");
  await ensureAssetSeeded(assetHub, ASSET_ID, alice, 1_000n * UNIT, "assetHub");
  await ensureAssetSeeded(assetHub, USDT_ID, alice, 100n * UNIT_USDT, "assetHub");

  console.log("Seeding local mirrors of asset 42 and USDT on Xode (MultiAssetMatcher requires the same id to exist locally) ...");
  await ensureAssetSeeded(xode, ASSET_ID, alice, 1_000n * UNIT, "xode");
  await ensureAssetSeeded(xode, USDT_ID, alice, 1n, "xode");

  const dest = { V5: { parents: 1, interior: { X1: [{ Parachain: XODE_PARA_ID }] } } };
  const sovereignOnAssetHub = (await assetHub.call.locationToAccountApi.convertLocation(dest)).asOk.toString();
  await ensureAccountFunded(assetHub, sovereignOnAssetHub, alice, 10n * UNIT, "Xode's sovereign account on AssetHub");
  await ensureAccountFunded(xode, FEE_COLLECTOR, alice, 1n * UNIT, "Xode's XCM fee collector");

  console.log("Ensuring the Native/Local(42) pool exists, is approved, and has liquidity on Xode ...");
  await ensurePoolReady(xode, alice, bob, charlie);

  console.log("\n=== Swap ===");
  const daveBefore = await xode.query.system.account(dave.address);
  console.log("Dave's native balance on Xode before:", daveBefore.data.free.toString());

  const asset42Location = { parents: 0, interior: { X2: [{ PalletInstance: 50 }, { GeneralIndex: ASSET_ID }] } };
  const usdtLocation = { parents: 0, interior: { X2: [{ PalletInstance: 50 }, { GeneralIndex: USDT_ID }] } };
  const reanchoredAsset42 = { parents: 1, interior: { X3: [{ Parachain: ASSETHUB_PARA_ID }, { PalletInstance: 50 }, { GeneralIndex: ASSET_ID }] } };
  const assets = { V5: [
    { id: asset42Location, fun: { Fungible: SWAP_AMOUNT } },
    { id: usdtLocation, fun: { Fungible: FEE_AMOUNT } },
  ] };
  const transferType = { LocalReserve: null };
  const remoteFeesId = { V5: usdtLocation };
  const customXcmOnDest = {
    V5: [
      {
        // `Definite`, not `Wild(All)`: holding also contains leftover USDT change after fees are
        // paid, and SingleAssetExchangeAdapter only accepts exactly one asset in `give`.
        ExchangeAsset: {
          give: { Definite: [{ id: reanchoredAsset42, fun: { Fungible: SWAP_AMOUNT } }] },
          want: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: 1 } }],
          maximal: false,
        },
      },
      {
        DepositAsset: {
          assets: { Wild: "All" },
          beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: dave.publicKey } }] } },
        },
      },
    ],
  };
  const weightLimit = "Unlimited";

  console.log("Sending asset 42 from AssetHub to Xode with an ExchangeAsset tail ...");
  await submit(
    assetHub,
    assetHub.tx.polkadotXcm.transferAssetsUsingTypeAndThen(
      dest, assets, transferType, remoteFeesId, transferType, customXcmOnDest, weightLimit,
    ),
    alice,
    "assetHub.polkadotXcm.transferAssetsUsingTypeAndThen",
  );

  console.log("Waiting for the message to be processed on Xode ...");
  let delivered = false;
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 6000));
    const daveAfter = await xode.query.system.account(dave.address);
    if (daveAfter.data.free.gt(daveBefore.data.free)) {
      console.log("Dave's native balance on Xode after:", daveAfter.data.free.toString());
      delivered = true;
      break;
    }
  }
  if (!delivered) {
    throw new Error("Dave's native balance on Xode never increased - check xcm=trace logs on both nodes.");
  }
  console.log("Cross-chain swap-and-forward succeeded.");

  await Promise.all([relay.disconnect(), assetHub.disconnect(), xode.disconnect()]);
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
