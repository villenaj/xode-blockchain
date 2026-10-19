import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";

const XODE_WS = "ws://127.0.0.1:14144";
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

  // The REAL bridged Ether location, already live on this Snowbridge stack's AssetHub.
  const bridgedEthLocation = { parents: 2, interior: { X1: [{ GlobalConsensus: { Ethereum: { chainId: 11155111 } } }] } };
  // The REAL reserve this time: AssetHub itself (para 1000), not a synthetic local account.
  const reserveLocation = { parents: 1, interior: { X1: [{ Parachain: 1000 }] } };
  // assetRegistry.locationToAssetId's storage key is XCM v3 Location internally (confirmed from
  // TokensAdapter::id_of's v5->v4->v3 conversion in the runtime source) - v3's X1 holds a bare
  // Junction, not the 1-element array v5 uses. The extrinsic's `metadata.location` field is a
  // proper VersionedLocation though, so it keeps the v5 array form.
  const bridgedEthLocationV3Query = { parents: 2, interior: { X1: { GlobalConsensus: { Ethereum: { chainId: 11155111 } } } } };

  console.log("\n=== 1. Register bridged Ether via TreasuryCouncil governance (reserve = real AssetHub) ===");
  const already = await xode.query.assetRegistry.locationToAssetId(bridgedEthLocationV3Query);
  if (already.isSome) {
    console.log("Already registered as ForeignAssetId", already.unwrap().toNumber(), "- skipping.");
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
  }

  const assignedId = await xode.query.assetRegistry.locationToAssetId(bridgedEthLocationV3Query);
  if (assignedId.isNone) throw new Error("Asset registration did not take effect");
  const foreignId = assignedId.unwrap().toNumber();
  console.log("Registered as ForeignAssetId:", foreignId);
  const foreignKind = { Foreign: foreignId };

  console.log(`\n=== 2. Create + approve the Native/Foreign(${foreignId}) pool (liquidity added once real ETH arrives) ===`);
  const poolDetails = await xode.query.assetWaiting.poolDetailsById([{ Native: null }, foreignKind]);
  if (poolDetails.isNone) {
    await submit(xode, xode.tx.assetWaiting.createPool({ Native: null }, foreignKind), alice, "createPool");
    await submit(xode, xode.tx.assetWaiting.approvePool({ Native: null }, foreignKind), bob, "approvePool(Bob)");
    await submit(xode, xode.tx.assetWaiting.approvePool({ Native: null }, foreignKind), charlie, "approvePool(Charlie)");
    console.log("Pool created and approved (Active). Liquidity will be added once real bridged ETH lands on Xode.");
  } else {
    console.log("Pool already exists, status:", poolDetails.unwrap().status.toString());
  }

  await xode.disconnect();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
