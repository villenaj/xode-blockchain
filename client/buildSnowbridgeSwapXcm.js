// Builds the SCALE-encoded `xcm` bytes for Snowbridge V2's `IGatewayV2.v2_sendMessage`, for a
// "one Ethereum signature" USDT -> XON swap: the user calls v2_sendMessage on Ethereum once,
// Snowbridge relayers deliver it to AssetHub with the bridged USDT already in the holding
// register, and THIS program (running on AssetHub) pays its own fee, forwards the USDT to Xode
// via a reserve-transfer, and Xode's own copy of the tail runs ExchangeAsset (GatedAssetExchanger)
// + DepositAsset to the user - no second signature anywhere.
//
// PROVEN vs DESIGN, please read before using this for real funds:
//   - The Xode-side tail (BuyExecution -> ExchangeAsset -> DepositAsset) is EXACTLY what
//     client/testCrossChainSwap.js already ran successfully end-to-end on a live network - same
//     shape, same GatedAssetExchanger codepath.
//   - The AssetHub-side wrapper (BuyExecution -> DepositReserveAsset) is a well-grounded design
//     based on Snowbridge's documented IGatewayV2.sol interface (see the doc comment quoted
//     below), but has NOT been run against a live Snowbridge/Ethereum testnet - there's no bridge
//     in this local zombienet setup to test it against. Treat this file as a verified *encoding*
//     (it does successfully build against AssetHub's real XCM v5 metadata) and a *design*, not a
//     confirmed-working flow the way testCrossChainSwap.js is.
//   - Two things specifically need confirming against a real Snowbridge deployment before this
//     goes anywhere near production: (1) exactly which asset Snowbridge deposits into AssetHub's
//     holding register to cover `executionFee` (see FEE_ASSET_MODE below - I don't have source
//     confirmation of this), and (2) the bridged USDT's exact registered Location on AssetHub
//     (used here as a best-guess based on Snowbridge's Ethereum-asset addressing convention).
//
// Usage:
//   ASSETHUB_WS=ws://127.0.0.1:9955 node client/buildSnowbridgeSwapXcm.js

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import 'dotenv/config';

const ASSETHUB_WS = process.env.ASSETHUB_WS ?? "ws://127.0.0.1:9955";
const XODE_PARA_ID = 3417;

// Real Ethereum mainnet USDT contract. Swap for your target token/chainId as needed.
const ETHEREUM_CHAIN_ID = 1;
const USDT_ERC20_ADDRESS = "0xdAC17F958D2ee523a2206206994597C13D831ec7";

// How Snowbridge V2 registers a bridged Ethereum asset's Location on AssetHub - a
// GlobalConsensus(Ethereum) + AccountKey20(contract address) pair, per Snowbridge's asset
// registration convention. Confirm the exact Location against `v2_registerToken`'s on-chain
// result / AssetHub's ForeignAssets registry before relying on this for real transfers.
const bridgedUsdtOnAssetHub = {
  parents: 2,
  interior: {
    X2: [
      { GlobalConsensus: { Ethereum: { chainId: ETHEREUM_CHAIN_ID } } },
      { AccountKey20: { network: null, key: USDT_ERC20_ADDRESS } },
    ],
  },
};

// Set to "usdt" to pay AssetHub's own execution fee out of the bridged USDT itself (needs
// AssetHub's fee trader / asset-conversion-tx-payment to accept it - true for real USDT-1984,
// unconfirmed for a *bridged* ERC20 representation). Set to "eth" if Snowbridge deposits a
// separate bridged-ETH/WETH asset into holding to cover `executionFee` - the safer assumption
// per the "either from assets in holding" wording in IGatewayV2.sol, but the exact asset id
// needs confirming against a live deployment.
const FEE_ASSET_MODE = "usdt";

function buildTailOnXode({ swapAmount, minWantAmount, beneficiaryAccountId32 }) {
  // Identical in shape to the customXcmOnDest that client/testCrossChainSwap.js already proved
  // works, just with `reanchoredAsset` swapped for the bridged-USDT location and `want`
  // targeting native XON (Location::here() once this runs ON Xode).
  return [
    {
      ExchangeAsset: {
        // Definite, not Wild(All): holding may still contain leftover fee-asset change.
        give: { Definite: [{ id: bridgedUsdtOnAssetHub, fun: { Fungible: swapAmount } }] },
        want: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: minWantAmount } }],
        maximal: false,
      },
    },
    {
      DepositAsset: {
        assets: { Wild: "All" },
        beneficiary: { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: beneficiaryAccountId32 } }] } },
      },
    },
  ];
}

function buildAssetHubProgram({ swapAmount, feeAmountOnAssetHub, minWantAmount, beneficiaryAccountId32 }) {
  const xodeTail = buildTailOnXode({ swapAmount, minWantAmount, beneficiaryAccountId32 });

  const feeAsset =
    FEE_ASSET_MODE === "usdt"
      ? { id: bridgedUsdtOnAssetHub, fun: { Fungible: feeAmountOnAssetHub } }
      // Placeholder - replace with whatever Location Snowbridge actually deposits for `executionFee`.
      : { id: { parents: 0, interior: "Here" }, fun: { Fungible: feeAmountOnAssetHub } };

  return {
    V5: [
      // Pay AssetHub's own cost of running the rest of this program.
      { BuyExecution: { fees: feeAsset, weightLimit: "Unlimited" } },
      // Standard XCM instruction: moves `assets` from holding into Xode's sovereign account on
      // AssetHub (same account client/testCrossChainSwap.js pre-funds), then SENDS Xode a message
      // of the form `ReserveAssetDeposited(reanchored assets) -> ClearOrigin -> xcm` - this is
      // the spec-defined behaviour of DepositReserveAsset, and it's exactly the message shape
      // our GatedAssetExchanger test already consumed successfully (minus us having to build the
      // ReserveAssetDeposited/ClearOrigin prefix by hand - the executor does it here).
      {
        DepositReserveAsset: {
          assets: { Wild: "All" },
          dest: { parents: 1, interior: { X1: [{ Parachain: XODE_PARA_ID }] } },
          xcm: xodeTail,
        },
      },
    ],
  };
}

async function main() {
  await cryptoWaitReady();
  // Example beneficiary: //Dave's public key (32 bytes). Replace with the real user's Xode
  // account - derived via Keyring rather than hand-typed, to not repeat the exact mistake this
  // draft already made twice with hand-transcribed hex (the USDT address, then this key).
  const dave = new Keyring({ type: "sr25519" }).addFromUri("//Dave");

  console.log(`Connecting to AssetHub ${ASSETHUB_WS} to validate encoding against real XCM v5 metadata ...`);
  const api = await ApiPromise.create({ provider: new WsProvider(ASSETHUB_WS) });

  const UNIT = 1_000_000_000_000n;
  const program = buildAssetHubProgram({
    swapAmount: 10n * UNIT,
    feeAmountOnAssetHub: 1n * UNIT,
    minWantAmount: 1n,
    beneficiaryAccountId32: dave.publicKey,
  });

  console.log("\nConstructed program (V5):");
  console.log(JSON.stringify(program, (_k, v) => (typeof v === "bigint" ? v.toString() : v), 2));

  const encoded = api.createType("XcmVersionedXcm", program);
  const bytes = encoded.toU8a();
  console.log(`\nEncodes cleanly against AssetHub's live metadata: ${bytes.length} bytes.`);
  console.log("xcm (hex, pass this as v2_sendMessage's `xcm` param):");
  console.log(encoded.toHex());

  // The `assets` param for v2_sendMessage: per IGatewayV2.sol, ERC20 specs are
  // `abi.encode(0, tokenAddress, value)`. Building the actual bytes needs an ABI encoder
  // (ethers.js `AbiCoder`), not @polkadot/api - shown here as pseudocode:
  //
  //   import { AbiCoder } from "ethers";
  //   const assetSpec = AbiCoder.defaultAbiCoder().encode(
  //     ["uint8", "address", "uint128"],
  //     [0, USDT_ERC20_ADDRESS, swapAmountInUsdtDecimals],
  //   );
  //
  // Then, on the Ethereum side (ethers.js, wagmi, viem - whichever the DEX app uses):
  //
  //   await gateway.v2_sendMessage(
  //     encoded.toHex(),           // xcm, from this script
  //     [assetSpec],               // assets
  //     claimerBytes,               // SCALE-encoded XCM Location of a claimer account (refund path)
  //     executionFee,               // in wei, covers AssetHub-side BuyExecution
  //     relayerFee,                 // in wei, relayer incentive
  //     { value: totalEthValue },   // executionFee + relayerFee (+ any bridging base fee)
  //   );

  await api.disconnect();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
