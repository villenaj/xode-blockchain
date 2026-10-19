import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

const ASSETHUB_WS = "ws://127.0.0.1:12144";
const XODE_WS = "ws://127.0.0.1:14144";
const XODE_PARA_ID = 3417;
const FEE_ASSET_ID = 7777;
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

async function ensureFeeAssetSeeded(api, signer, mintAmount, label) {
  const existing = await api.query.assets.asset(FEE_ASSET_ID);
  if (existing.isNone) {
    await submit(api, api.tx.assets.create(FEE_ASSET_ID, signer.address, 1), signer, `${label}.assets.create`);
  }
  const balance = await api.query.assets.account(FEE_ASSET_ID, signer.address);
  const have = balance.isSome ? BigInt(balance.unwrap().balance.toString()) : 0n;
  if (have < mintAmount) {
    await submit(api, api.tx.assets.mint(FEE_ASSET_ID, signer.address, mintAmount), signer, `${label}.assets.mint`);
  }
}

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: "sr25519" });
  const alice = keyring.addFromUri("//Alice");
  const bob = keyring.addFromUri("//Bob");

  const ah = await ApiPromise.create({ provider: new WsProvider(ASSETHUB_WS) });
  const xode = await ApiPromise.create({ provider: new WsProvider(XODE_WS) });

  console.log("=== Seeding the fee-paying asset (id 7777) on both chains ===");
  await ensureFeeAssetSeeded(ah, alice, 1000n * UNIT, "assetHub");
  await ensureFeeAssetSeeded(xode, alice, 1n, "xode");

  const bridgedEthLocation = { parents: 2, interior: { X1: [{ GlobalConsensus: { Ethereum: { chainId: 11155111 } } }] } };
  const feeAssetLocation = { parents: 0, interior: { X2: [{ PalletInstance: 50 }, { GeneralIndex: FEE_ASSET_ID }] } };
  const dest = { V5: { parents: 1, interior: { X1: [{ Parachain: XODE_PARA_ID }] } } };
  const transferType = { LocalReserve: null };
  const weightLimit = "Unlimited";

  function buildAssets(ethAmount, feeAmount) {
    return { V5: [
      { id: bridgedEthLocation, fun: { Fungible: ethAmount } },
      { id: feeAssetLocation, fun: { Fungible: feeAmount } },
    ] };
  }

  console.log("\n=== Step A: plain transfer to fund Alice on Xode with bridged ETH for liquidity ===");
  const fundAmount = 500_000_000_000_000n; // 5e14 wei-ish units of the 18-decimal bridged ETH
  const feeAmount = 10n * UNIT;
  const plainTail = { V5: [
    { DepositAsset: { assets: { Wild: "All" }, beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: alice.publicKey } }] } } } },
  ] };
  await submit(
    ah,
    ah.tx.polkadotXcm.transferAssetsUsingTypeAndThen(dest, buildAssets(fundAmount, feeAmount), transferType, { V5: feeAssetLocation }, transferType, plainTail, weightLimit),
    alice,
    "fund transfer",
  );

  console.log("\n=== Step B: wait for Alice's Foreign(ETH) balance to arrive on Xode, then add liquidity ===");
  const foreignId = (await xode.query.assetRegistry.locationToAssetId({ parents: 2, interior: { X1: { GlobalConsensus: { Ethereum: { chainId: 11155111 } } } } })).unwrap().toNumber();
  console.log("bridged ETH is ForeignAssetId", foreignId, "on Xode");
  let aliceEthBalance = 0n;
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 6000));
    const bal = await xode.query.tokens.accounts(alice.address, foreignId);
    aliceEthBalance = BigInt(bal.free.toString());
    console.log("poll", i, "- Alice's Foreign(ETH) balance on Xode:", aliceEthBalance.toString());
    if (aliceEthBalance > 0n) break;
  }
  if (aliceEthBalance === 0n) throw new Error("Funding transfer never arrived on Xode");

  const liquidityAmount = aliceEthBalance / 2n;
  await submit(
    xode,
    xode.tx.assetConversion.addLiquidity({ Native: null }, { Foreign: foreignId }, 100n * UNIT, liquidityAmount, 1, 1, alice.address),
    alice,
    "addLiquidity",
  );

  console.log("\n=== Step C: the real swap-and-forward test ===");
  const bobBefore = await xode.query.system.account(bob.address);
  console.log("Bob's native balance before:", bobBefore.data.free.toString());

  const swapAmount = 100_000_000_000_000n;
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
  await submit(
    ah,
    ah.tx.polkadotXcm.transferAssetsUsingTypeAndThen(dest, buildAssets(swapAmount, feeAmount), transferType, { V5: feeAssetLocation }, transferType, swapTail, weightLimit),
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
