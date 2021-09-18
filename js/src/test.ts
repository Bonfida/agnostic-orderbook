import { MarketState } from "./market_state";
// import { find_max } from "dex-wasm";
// import { Slab } from "./slab";
import {
  Connection,
  //   Keypair,
  //   LAMPORTS_PER_SOL,
  PublicKey,
  //   Transaction,
} from "@solana/web3.js";
import { EventQueue } from "./event_queue";
// import { deserialize } from "borsh";

const URL = "https://api.devnet.solana.com";

const connection = new Connection(URL);

const test = async () => {
  // Load market
  const market = await MarketState.retrieve(
    connection,
    new PublicKey("G2pbv4RtDpaygMELxbDQpWjedw4j1ujKNnEiFLsmhNUy")
  );

  // let bids_pubkey = market.bids;
  // console.log(bids_pubkey.toString());
  // let bids_data = (await connection.getAccountInfo(bids_pubkey))?.data;
  // if (!bids_data) throw "d";
  // let bids_slab = await market.loadBidsSlab(connection);
  // bids_slab.data = bids_data;

  // find_max(
  //   bids_data,
  //   BigInt(bids_slab.callBackInfoLen),
  //   BigInt(bids_slab.slotSize)
  // );

  let eq_p = market.eventQueue;
  let eq_data = await connection.getAccountInfo(eq_p);
  if (!eq_data) throw "d";
  let eq = EventQueue.parse(33, eq_data.data);
  console.log(eq.parseEvent(0));
};
test();
