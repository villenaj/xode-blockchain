import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

const ASSETHUB_WS = "ws://127.0.0.1:12144";
const XODE_WS = "ws://127.0.0.1:14144";
const XODE_PARA_ID = 3417;
const UNIT = 1_000_000_000_000n;

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

async function runTreasuryCouncilMotion(xode, call, members) {
  const lengthBound = call.encodedLength + 10;
  await submit(xode, xode.tx.treasuryCouncil.propose(2, call, lengthBound), members[0], "treasuryCouncil.propose");
  const proposals = await xode.query.treasuryCouncil.proposals();
  const proposalHash = proposals[proposals.length - 1];
  const proposalIndex = (await xode.query.treasuryCouncil.voting(proposalHash)).unwrap().index.toNumber();
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

  const ah = await ApiPromise.create({ provider: new WsProvider(ASSETHUB_WS) });
  const xode = await ApiPromise.create({ provider: new WsProvider(XODE_WS) });

  const bridgedEthLocation = { parents: 2, interior: { X1: [{ GlobalConsensus: { Ethereum: { chainId: 11155111 } } }] } };
  const bridgedEthLocationV3Query = { parents: 2, interior: { X1: { GlobalConsensus: { Ethereum: { chainId: 11155111 } } } } };
  const reserveLocation = { parents: 1, interior: { X1: [{ Parachain: 1000 }] } };
  const dest = { V5: { parents: 1, interior: { X1: [{ Parachain: XODE_PARA_ID }] } } };
  const transferType = { LocalReserve: null };
  const weightLimit = "Unlimited";

  console.log("=== 1. Fund Xode's sovereign account on AssetHub ===");
  const sovereignAccount = "5Eg2fnstBR7aoMFirfxStY1AuK7aAopL9UyDS3EfDnAsePS4";
  const sState = await ah.query.system.account(sovereignAccount);
  if (sState.providers.toNumber() === 0) {
    await submit(ah, ah.tx.balances.transferKeepAlive(sovereignAccount, 10_000_000_000_000n), alice, "fund sovereign account");
  } else {
    console.log("already funded");
  }

  console.log("\n=== 2. Register bridged Ether on Xode (reserve = real AssetHub) ===");
  const already = await xode.query.assetRegistry.locationToAssetId(bridgedEthLocationV3Query);
  let foreignId;
  if (already.isSome) {
    foreignId = already.unwrap().toNumber();
    console.log("already registered as ForeignAssetId", foreignId);
  } else {
    const metadata = {
      decimals: 18,
      name: "Bridged Ether",
      symbol: "ETH.snow",
      existentialDeposit: 1,
      location: { V5: bridgedEthLocation },
      additional: { reserve: reserveLocation },
    };
    await runTreasuryCouncilMotion(xode, xode.tx.assetRegistry.registerAsset(metadata, null), [dave, eve, ferdie]);
    foreignId = (await xode.query.assetRegistry.locationToAssetId(bridgedEthLocationV3Query)).unwrap().toNumber();
    console.log("registered as ForeignAssetId", foreignId);
  }
  const foreignKind = { Foreign: foreignId };

  console.log(`\n=== 3. Create + approve the Native/Foreign(${foreignId}) pool ===`);
  const poolDetails = await xode.query.assetWaiting.poolDetailsById([{ Native: null }, foreignKind]);
  if (poolDetails.isNone) {
    await submit(xode, xode.tx.assetWaiting.createPool({ Native: null }, foreignKind), alice, "createPool");
    await submit(xode, xode.tx.assetWaiting.approvePool({ Native: null }, foreignKind), bob, "approvePool(Bob)");
    await submit(xode, xode.tx.assetWaiting.approvePool({ Native: null }, foreignKind), charlie, "approvePool(Charlie)");
  } else {
    console.log("pool already exists, status:", poolDetails.unwrap().status.toString());
  }

  console.log("\n=== 4. Fund Alice on Xode with bridged ETH (plain transfer, ETH pays its own way) ===");
  const fundAmount = 1_000_000_000_000_000_000n; // 1 ETH
  const plainTail = { V5: [
    { DepositAsset: { assets: { Wild: "All" }, beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: alice.publicKey } }] } } } },
  ] };
  await submit(
    ah,
    ah.tx.polkadotXcm.transferAssetsUsingTypeAndThen(
      dest, { V5: [{ id: bridgedEthLocation, fun: { Fungible: fundAmount } }] }, transferType, { V5: bridgedEthLocation }, transferType, plainTail, weightLimit,
    ),
    alice,
    "fund transfer",
  );

  let aliceEthBalance = 0n;
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 6000));
    const bal = await xode.query.tokens.accounts(alice.address, foreignId);
    aliceEthBalance = BigInt(bal.free.toString());
    console.log("poll", i, "- Alice's Foreign(ETH) balance on Xode:", aliceEthBalance.toString());
    if (aliceEthBalance > 0n) break;
  }
  if (aliceEthBalance === 0n) throw new Error("Funding transfer never arrived on Xode");

  console.log("\n=== 5. Add liquidity ===");
  const liquidityAmount = aliceEthBalance / 2n;
  await submit(
    xode,
    xode.tx.assetConversion.addLiquidity({ Native: null }, foreignKind, 100n * UNIT, liquidityAmount, 1, 1, alice.address),
    alice,
    "addLiquidity",
  );

  console.log("\n=== 6. The real swap-and-forward test (ETH pays its own way end to end) ===");
  const bobBefore = await xode.query.system.account(bob.address);
  console.log("Bob's native balance before:", bobBefore.data.free.toString());

  const swapAmount = 100_000_000_000_000_000n; // 0.1 ETH
  const swapTail = { V5: [
    {
      ExchangeAsset: {
        give: { Definite: [{ id: bridgedEthLocation, fun: { Fungible: swapAmount } }] },
        want: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: 1 } }],
        maximal: false,
      },
    },
    { DepositAsset: { assets: { Wild: "All" }, beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: bob.publicKey } }] } } } },
  ] };
  const totalSend = swapAmount + fundAmount / 10n; // swap amount + a chunk for Xode-side fees
  await submit(
    ah,
    ah.tx.polkadotXcm.transferAssetsUsingTypeAndThen(
      dest, { V5: [{ id: bridgedEthLocation, fun: { Fungible: totalSend } }] }, transferType, { V5: bridgedEthLocation }, transferType, swapTail, weightLimit,
    ),
    alice,
    "swap transfer",
  );

  console.log("Watching Xode for delivery ...");
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 6000));
    const bobAfter = await xode.query.system.account(bob.address);
    console.log("poll", i, "- Bob's native balance:", bobAfter.data.free.toString());
    if (bobAfter.data.free.gt(bobBefore.data.free)) {
      console.log("\nSUCCESS: real cross-chain swap-and-forward, backed by a genuine Snowbridge-style bridge, delivered!");
      await Promise.all([ah.disconnect(), xode.disconnect()]);
      process.exit(0);
    }
  }
  throw new Error("Swap did not deliver - check xcm=trace logs on both nodes.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
