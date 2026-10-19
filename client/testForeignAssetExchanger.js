// Validates the OTHER asset-recognition pathway GatedAssetExchanger can go through - the one
// real bridged assets (e.g. Ethereum USDT via Snowbridge) would actually use.
//
// client/testCrossChainSwap.js already proved the MultiAssetMatcher pathway: an asset hardcoded
// as "known" (native XON, relay DOT, or any AssetHub PalletInstance(50) asset) gets recognized
// without any registration. Real bridged assets don't get that shortcut - they arrive at an
// arbitrary Location (e.g. `{parents:2, interior: GlobalConsensus(Ethereum) + AccountKey20(...)}`
// for Snowbridge-bridged Ethereum tokens) that only `ForeignAssetMatcher` recognizes, and only
// after governance registers it in `AssetRegistry`. This script exercises exactly that pathway,
// using the real Ethereum USDT contract's Location as the registered asset, so the only
// non-authentic part is *how* the asset arrives.
//
// Real cross-chain arrival needs a message whose origin matches the asset's registered `reserve`
// Location - there's no live bridge to send that from in this local zombienet (see the
// conversation this script came out of: Xode isn't deployed on any chain Snowbridge actually
// bridges to). So this registers Alice's own local Xode account as the trusted reserve and uses
// `pallet_xcm.execute` to submit the exact XCM program a real arrival would run
// (ReserveAssetDeposited -> ClearOrigin -> BuyExecution -> ExchangeAsset -> DepositAsset) signed
// by her. XCM's trust model only checks "does this message's origin match the registered
// reserve" - it doesn't know or care that the origin is a plain local account instead of a real
// remote chain - so this exercises the identical runtime code (AssetRegistry lookup,
// ForeignAssetMatcher, Tokens, GatedAssetExchanger) a real arrival would.
//
// Usage:
//   XODE_WS=ws://127.0.0.1:9977 node client/testForeignAssetExchanger.js

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import 'dotenv/config';

const XODE_WS = process.env.XODE_WS ?? "ws://127.0.0.1:9977";
const UNIT = 1_000_000_000_000n;
// Real Tether USD contract on Ethereum mainnet (verified byte-for-byte against Uniswap's
// canonical token list - see the conversation this came from for why that verification mattered).
const USDT_ERC20_ADDRESS = "0xdAC17F958D2ee523a2206206994597C13D831ec7";

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

// Runs a TreasuryCouncil motion to completion. AssetsForceOrigin = EnsureProportionMoreThan<2,3>
// needs STRICTLY more than 2/3 - with exactly 3 members that means all 3 ayes, not just the
// motion's own 2-of-3 pass threshold (which only governs whether `close` may execute at all).
// The proposer doesn't get an implicit aye, so every member votes explicitly.
async function runTreasuryCouncilMotion(xode, call, members) {
  const lengthBound = call.encodedLength + 10;
  await submit(xode, xode.tx.treasuryCouncil.propose(2, call, lengthBound), members[0], "treasuryCouncil.propose");

  const proposals = await xode.query.treasuryCouncil.proposals();
  const proposalHash = proposals[proposals.length - 1];
  const proposalIndex = (await xode.query.treasuryCouncil.voting(proposalHash)).unwrap().index.toNumber();
  console.log("proposal hash:", proposalHash.toHex(), "index:", proposalIndex);

  for (const member of members) {
    await submit(xode, xode.tx.treasuryCouncil.vote(proposalHash, proposalIndex, true), member, `treasuryCouncil.vote(${member.address})`);
  }
  await submit(
    xode,
    xode.tx.treasuryCouncil.close(proposalHash, proposalIndex, { refTime: 2_000_000_000n, proofSize: 200_000n }, lengthBound),
    members[members.length - 1],
    "treasuryCouncil.close",
  );
}

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");
  const bob = keyring.addFromUri("//Bob");
  const charlie = keyring.addFromUri("//Charlie");
  const dave = keyring.addFromUri("//Dave");
  const eve = keyring.addFromUri("//Eve");
  const ferdie = keyring.addFromUri("//Ferdie");

  console.log(`Connecting to Xode ${XODE_WS} ...`);
  const xode = await ApiPromise.create({ provider: new WsProvider(XODE_WS) });

  const bridgedUsdtLocation = {
    parents: 2,
    interior: { X2: [
      { GlobalConsensus: { Ethereum: { chainId: 1 } } },
      { AccountKey20: { network: null, key: USDT_ERC20_ADDRESS } },
    ] },
  };
  const reserveLocation = { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: alice.publicKey } }] } };

  console.log("\n=== 1. Register the bridged-USDT Location via TreasuryCouncil governance ===");
  const already = await xode.query.assetRegistry.locationToAssetId(bridgedUsdtLocation);
  if (already.isSome) {
    console.log("Already registered as ForeignAssetId", already.unwrap().toNumber(), "- skipping.");
  } else {
    const metadata = {
      decimals: 6,
      name: "Ethereum USDT (local test)",
      symbol: "USDT.e",
      existentialDeposit: 1,
      location: { V5: bridgedUsdtLocation },
      additional: { reserve: reserveLocation },
    };
    await runTreasuryCouncilMotion(xode, xode.tx.assetRegistry.registerAsset(metadata, null), [dave, eve, ferdie]);
  }

  const assignedId = await xode.query.assetRegistry.locationToAssetId(bridgedUsdtLocation);
  if (assignedId.isNone) throw new Error("Asset registration did not take effect");
  const foreignId = assignedId.unwrap().toNumber();
  console.log("Registered as ForeignAssetId:", foreignId);
  const foreignKind = { Foreign: foreignId };
  const maxWeight = { refTime: 20_000_000_000n, proofSize: 2_000_000n };

  console.log("\n=== 2. Bootstrap: simulate an initial arrival to fund Alice for liquidity ===");
  const bootstrapMsg = { V5: [
    { ReserveAssetDeposited: [{ id: bridgedUsdtLocation, fun: { Fungible: 1000n * UNIT } }] },
    "ClearOrigin",
    { BuyExecution: { fees: { id: { parents: 0, interior: "Here" }, fun: { Fungible: 1n * UNIT } }, weightLimit: "Unlimited" } },
    { DepositAsset: { assets: { Wild: "All" }, beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: alice.publicKey } }] } } } },
  ] };
  await submit(xode, xode.tx.polkadotXcm.execute(bootstrapMsg, maxWeight), alice, "bootstrap execute");
  const aliceForeignBal = await xode.query.tokens.accounts(alice.address, foreignId);
  console.log(`Alice's Foreign(${foreignId}) balance after bootstrap:`, aliceForeignBal.toHuman());

  console.log(`\n=== 3. Create + approve + fund the Native/Foreign(${foreignId}) pool ===`);
  const poolDetails = await xode.query.assetWaiting.poolDetailsById([{ Native: null }, foreignKind]);
  if (poolDetails.isNone) {
    await submit(xode, xode.tx.assetWaiting.createPool({ Native: null }, foreignKind), alice, "createPool");
    await submit(xode, xode.tx.assetWaiting.approvePool({ Native: null }, foreignKind), bob, "approvePool(Bob)");
    await submit(xode, xode.tx.assetWaiting.approvePool({ Native: null }, foreignKind), charlie, "approvePool(Charlie)");
  } else {
    console.log("Pool already exists, status:", poolDetails.unwrap().status.toString());
  }
  await submit(
    xode,
    xode.tx.assetConversion.addLiquidity({ Native: null }, foreignKind, 100n * UNIT, 100n * UNIT, 1, 1, alice.address),
    alice,
    "addLiquidity",
  );

  console.log("\n=== 4. The real test: genuine reserve-arrival -> ExchangeAsset -> DepositAsset ===");
  const bobBefore = await xode.query.system.account(bob.address);
  console.log("Bob's native balance before:", bobBefore.data.free.toString());

  const swapAmount = 10n * UNIT;
  const swapMsg = { V5: [
    { ReserveAssetDeposited: [{ id: bridgedUsdtLocation, fun: { Fungible: swapAmount } }] },
    "ClearOrigin",
    { BuyExecution: { fees: { id: { parents: 0, interior: "Here" }, fun: { Fungible: 1n * UNIT } }, weightLimit: "Unlimited" } },
    {
      ExchangeAsset: {
        give: { Definite: [{ id: bridgedUsdtLocation, fun: { Fungible: swapAmount } }] },
        want: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: 1 } }],
        maximal: false,
      },
    },
    { DepositAsset: { assets: { Wild: "All" }, beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: bob.publicKey } }] } } } },
  ] };
  await submit(xode, xode.tx.polkadotXcm.execute(swapMsg, maxWeight), alice, "swap execute");

  const bobAfter = await xode.query.system.account(bob.address);
  console.log("Bob's native balance after:", bobAfter.data.free.toString());
  if (!bobAfter.data.free.gt(bobBefore.data.free)) {
    throw new Error("Bob's balance did not increase - swap did not deliver.");
  }
  console.log("\nSUCCESS: ForeignAssetMatcher/AssetRegistry/Tokens/GatedAssetExchanger pathway works end-to-end.");

  await xode.disconnect();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
