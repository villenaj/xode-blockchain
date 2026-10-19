import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { ethers } from "ethers";
import fs from "node:fs";

const GETH_RPC = "http://127.0.0.1:8545";
const ASSETHUB_WS = "ws://127.0.0.1:12144";
const XODE_WS = "ws://127.0.0.1:14144";
const XODE_PARA_ID = 3417;
const DEPLOYER_KEY = "0x4e9444a6efd6d42725a250b650a781da2737ea308c839eaccb0f7f3dbd2fea77";

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: "sr25519" });
  const ferdie = keyring.addFromUri("//Ferdie"); // fresh beneficiary, untouched by earlier tests

  console.log("=== Setup ===");
  const contracts = JSON.parse(fs.readFileSync("/tmp/snowbridge-v2/contracts.json", "utf8"));
  const gatewayAbi = contracts.contracts.Gateway.abi;
  const gatewayAddress = contracts.contracts.GatewayProxy.address;
  console.log("Gateway proxy:", gatewayAddress);

  const provider = new ethers.JsonRpcProvider(GETH_RPC);
  const wallet = new ethers.Wallet(DEPLOYER_KEY, provider);
  const balance = await provider.getBalance(wallet.address);
  console.log("Deployer address:", wallet.address, "balance:", ethers.formatEther(balance), "ETH");

  const ah = await ApiPromise.create({ provider: new WsProvider(ASSETHUB_WS) });
  const xode = await ApiPromise.create({ provider: new WsProvider(XODE_WS) });

  const bridgedEthLocation = { parents: 2, interior: { X1: [{ GlobalConsensus: { Ethereum: { chainId: 11155111 } } }] } };
  const beneficiaryLocation = { parents: 0, interior: { X1: [{ AccountId32: { network: null, id: ferdie.publicKey } }] } };

  const xodeFeeAmount = 20_000_000_000_000_000n; // 0.02 ETH, comfortably above our 0.01 ETH fixed fee
  const swapAmount = 100_000_000_000_000_000n; // 0.1 ETH
  const totalBridged = xodeFeeAmount + swapAmount;
  const executionFee = 1_500_000_000_000n;
  const relayerFee = 1_500_000_000_000n;
  const msgValue = executionFee + relayerFee + totalBridged;

  console.log("\n=== Building the XCM program (encoded against AssetHub's real metadata) ===");
  const program = { V5: [
    {
      DepositReserveAsset: {
        assets: { Wild: "All" },
        dest: { parents: 1, interior: { X1: [{ Parachain: XODE_PARA_ID }] } },
        xcm: [
          { BuyExecution: { fees: { id: bridgedEthLocation, fun: { Fungible: xodeFeeAmount } }, weightLimit: "Unlimited" } },
          {
            ExchangeAsset: {
              give: { Definite: [{ id: bridgedEthLocation, fun: { Fungible: swapAmount } }] },
              want: [{ id: { parents: 0, interior: "Here" }, fun: { Fungible: 1 } }],
              maximal: false,
            },
          },
          { DepositAsset: { assets: { Wild: "All" }, beneficiary: beneficiaryLocation } },
        ],
      },
    },
  ] };
  const encodedXcm = ah.createType("XcmVersionedXcm", program);
  const xcmBytes = encodedXcm.toHex();
  console.log("xcm bytes:", xcmBytes.length / 2 - 1, "bytes");

  const claimerLocation = ah.createType("StagingXcmV5Location", beneficiaryLocation);
  const claimerBytes = claimerLocation.toHex();
  console.log("claimer bytes:", claimerBytes);

  const ferdieBefore = await xode.query.system.account(ferdie.address);
  console.log("\nFerdie's native balance on Xode before:", ferdieBefore.data.free.toString());

  console.log("\n=== Sending the real Ethereum transaction (v2_sendMessage) ===");
  const gateway = new ethers.Contract(gatewayAddress, gatewayAbi, wallet);
  const tx = await gateway.v2_sendMessage(xcmBytes, [], claimerBytes, executionFee, relayerFee, {
    value: msgValue,
  });
  console.log("tx hash:", tx.hash);
  const receipt = await tx.wait();
  console.log("confirmed in block:", receipt.blockNumber, "status:", receipt.status);

  console.log("\nWatching Xode for delivery (waiting for the relayer to pick this up) ...");
  for (let i = 0; i < 40; i++) {
    await new Promise((r) => setTimeout(r, 10000));
    const ferdieAfter = await xode.query.system.account(ferdie.address);
    console.log("poll", i, "- Ferdie's native balance:", ferdieAfter.data.free.toString());
    if (ferdieAfter.data.free.gt(ferdieBefore.data.free)) {
      console.log("\n*** SUCCESS: triggered by a REAL Ethereum transaction, delivered as native XON on Xode! ***");
      await Promise.all([ah.disconnect(), xode.disconnect()]);
      process.exit(0);
    }
  }
  throw new Error("Never arrived - check relayer logs and xcm=trace on AssetHub/Xode.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
