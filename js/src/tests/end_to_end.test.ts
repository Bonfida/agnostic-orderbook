import { afterAll, beforeAll, expect, jest, test } from "@jest/globals";
import * as web3 from "@solana/web3.js";
import BN from "bn.js";
import { ChildProcess } from "child_process";

import { createMarket } from "../bindings";
import { newOrderInstruction } from "../instructions";
import { MarketState } from "../market_state";

import {
  airdropPayer,
  deployProgram,
  initializePayer,
  spawnLocalSolana,
} from "./test_utils";

// Global state initialized once in test startup and cleaned up at test
// teardown.
let solana: ChildProcess;
let connection: web3.Connection;
let feePayer: web3.Keypair;
let payerKeyFile: string;
let programId: web3.PublicKey;

beforeAll(async () => {
  solana = await spawnLocalSolana();
  connection = new web3.Connection("http://localhost:8899");
  [feePayer, payerKeyFile] = initializePayer();
  await airdropPayer(connection, feePayer.publicKey);
  programId = deployProgram(payerKeyFile);
});

afterAll(() => {
  if (solana !== undefined) {
    try {
      solana.kill();
    } catch (e) {
      console.log(e);
    }
  }
});

jest.setTimeout(200000);
test("create bids", async () => {
  const callerAuthority = new web3.Keypair();
  const [[eventQueue, bids, asks, market], instructions] = await createMarket(
    connection,
    callerAuthority.publicKey,
    new BN(10),
    new BN(5),
    100,
    50,
    new BN(20),
    feePayer.publicKey,
    programId
  );

  // Create market and confirm creation.
  {
    const tx = new web3.Transaction({ feePayer: feePayer.publicKey });
    tx.add(...instructions);
    let signers = [eventQueue, bids, asks, market];
    signers.unshift(feePayer);
    const createMarketSignature = await connection.sendTransaction(
      tx,
      signers,
      { skipPreflight: true }
    );
    await connection.confirmTransaction(createMarketSignature, "finalized");

    const marketState = await MarketState.retrieve(
      connection,
      market.publicKey,
      "finalized"
    );
    const bidsSlab = await marketState.loadBidsSlab(connection, "finalized");
    const asksSlab = await marketState.loadAsksSlab(connection, "finalized");
    expect(marketState.eventQueue.toString()).toBe(
      eventQueue.publicKey.toString()
    );
    expect(marketState.bids.toString()).toBe(bids.publicKey.toString());
    expect(marketState.asks.toString()).toBe(asks.publicKey.toString());
    expect(marketState.callBackInfoLen.toString()).toBe("10");
    expect(marketState.callBackIdLen.toString()).toBe("5");
    expect(marketState.minOrderSize.toString()).toBe("20");
    expect(marketState.feeBudget.toString()).toBe("0");
    expect(bidsSlab.callBackInfoLen.toNumber()).toBe(10);
    expect(bidsSlab.header.marketAddress.toString()).toBe(
      market.publicKey.toString()
    );
    expect(asksSlab.callBackInfoLen.toNumber()).toBe(10);
    expect(asksSlab.header.marketAddress.toString()).toBe(
      market.publicKey.toString()
    );
  }

  const sendBid = async (args: {
    maxBaseQty: BN;
    maxQuoteQty: BN;
    limitPrice: BN;
    side: number;
    matchLimit: BN;
    callBackInfo: Uint8Array;
    postOnly: number;
    postAllowed: number;
    selfTradeBehavior: number;
  }) => {
    await airdropPayer(connection, market.publicKey);
    const tx = new web3.Transaction({ feePayer: feePayer.publicKey });
    tx.add(
      new newOrderInstruction({
        maxBaseQty: args.maxBaseQty,
        maxQuoteQty: args.maxQuoteQty,
        limitPrice: args.limitPrice,
        side: args.side,
        matchLimit: args.matchLimit,
        callBackInfo: args.callBackInfo,
        postOnly: args.postOnly,
        postAllowed: args.postAllowed,
        selfTradeBehavior: args.selfTradeBehavior,
      }).getInstruction(
        programId,
        market.publicKey,
        eventQueue.publicKey,
        bids.publicKey,
        asks.publicKey,
        callerAuthority.publicKey
      )
    );
    const sendOrderSignature = await connection.sendTransaction(
      tx,
      [feePayer, callerAuthority],
      { skipPreflight: true }
    );
    await connection.confirmTransaction(sendOrderSignature, "finalized");
  };

  await sendBid({
    maxBaseQty: new BN(1000),
    maxQuoteQty: new BN(1),
    limitPrice: new BN(300),
    side: 0,
    matchLimit: new BN(100),
    callBackInfo: Uint8Array.from([1, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
    postOnly: 1,
    postAllowed: 0,
    selfTradeBehavior: 0,
  });

  await sendBid({
    maxBaseQty: new BN(2000),
    maxQuoteQty: new BN(2),
    limitPrice: new BN(100),
    side: 0,
    matchLimit: new BN(100),
    callBackInfo: Uint8Array.from([2, 0, 0, 0, 0, 0, 0, 0, 0, 2]),
    postOnly: 1,
    postAllowed: 0,
    selfTradeBehavior: 0,
  });

  await sendBid({
    maxBaseQty: new BN(3000),
    maxQuoteQty: new BN(3),
    limitPrice: new BN(200),
    side: 0,
    matchLimit: new BN(100),
    callBackInfo: Uint8Array.from([3, 0, 0, 0, 0, 0, 0, 0, 0, 3]),
    postOnly: 1,
    postAllowed: 0,
    selfTradeBehavior: 0,
  });

  // Create bids (intentionally submitted out of order)
  // $300: 1000
  // $200: 2000
  // $100: 3000
  // Assert iteration order on the bids.
  {
    const marketState = await MarketState.retrieve(
      connection,
      market.publicKey,
      "finalized"
    );
    const bidsSlab = await marketState.loadBidsSlab(connection, "finalized");
    let prices: number[] = [];
    let quantities: number[] = [];
    let infos: Uint8Array[] = [];
    for (const bid of bidsSlab.items(false)) {
      prices.push(bid.getPrice().toNumber());
      quantities.push(bid.baseQuantity.toNumber());
      infos.push(bidsSlab.getCallBackInfo(bid.callBackInfoPt));
    }

    expect(prices).toStrictEqual([100, 200, 300]);
    expect(quantities).toStrictEqual([2000, 3000, 1000]);
    expect(infos).toStrictEqual([
      Buffer.from([2, 0, 0, 0, 0, 0, 0, 0, 0, 2]),
      Buffer.from([3, 0, 0, 0, 0, 0, 0, 0, 0, 3]),
      Buffer.from([1, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
    ]);

    const bestOrder = bidsSlab.getMinMaxNodes(1, true);
    expect(bestOrder.length).toBe(1);
    expect(bestOrder[0].getPrice().toNumber()).toBe(300);
  }

  // Create an order for the same best price level and check aggregation.

  await sendBid({
    maxBaseQty: new BN(1000),
    maxQuoteQty: new BN(1),
    limitPrice: new BN(300),
    side: 0,
    matchLimit: new BN(100),
    callBackInfo: Uint8Array.from([1, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
    postOnly: 1,
    postAllowed: 0,
    selfTradeBehavior: 0,
  });

  {
    const marketState = await MarketState.retrieve(
      connection,
      market.publicKey,
      "finalized"
    );
    const bidsSlab = await marketState.loadBidsSlab(connection, "finalized");
    const depth = bidsSlab.getL2DepthJS(1, false);

    expect(depth.length).toBe(1);
    expect(depth[0].price).toBe(300);
    expect(depth[0].size).toBe(2000);
  }
});
