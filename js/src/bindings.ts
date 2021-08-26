import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserializeUnchecked, deserialize } from "borsh";
import BN from "bn.js";

export class MarketState {
  static LEN = 1 + 32 + 32 + 32 + 32 + 8;
  accountFlags: number;
  callerAuthority: PublicKey;
  eventQueue: PublicKey;
  bids: PublicKey;
  asks: PublicKey;
  callBackInfoLen: BN;

  static schema: Schema = new Map([
    [
      MarketState,
      {
        kind: "struct",
        fields: [
          ["accountFlags", "u8"],
          ["callerAuthority", [32]],
          ["eventQueue", [32]],
          ["bids", [32]],
          ["asks", [32]],
          ["callBackInfoLen", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    accountFlags: number;
    callerAuthority: PublicKey;
    eventQueue: PublicKey;
    bids: PublicKey;
    asks: PublicKey;
    callBackInfoLen: BN;
  }) {
    this.accountFlags = arg.accountFlags;
    this.callerAuthority = arg.callerAuthority;
    this.eventQueue = arg.eventQueue;
    this.bids = arg.bids;
    this.asks = arg.asks;
    this.callBackInfoLen = arg.callBackInfoLen;
  }

  static retrieve = async (connection: Connection, market: PublicKey) => {
    const accountInfo = await connection.getAccountInfo(market);
    if (!accountInfo?.data) {
      throw new Error("Invalid account provided");
    }
    const marketState: MarketState = deserialize(
      this.schema,
      MarketState,
      accountInfo.data
    );
    return marketState;
  };
}

export class RequestProceeds {
  nativePcUnlocked: BN;
  coinCredit: BN;
  nativePcCredit: BN;
  coinDebit: BN;
  nativePcDebit: BN;

  constructor(arg: {
    nativePcUnlocked: BN;
    coinCredit: BN;
    nativePcCredit: BN;
    coinDebit: BN;
    nativePcDebit: BN;
  }) {
    this.nativePcUnlocked = arg.nativePcUnlocked;
    this.coinCredit = arg.coinCredit;
    this.nativePcCredit = arg.nativePcCredit;
    this.coinDebit = arg.coinDebit;
    this.nativePcDebit = arg.nativePcDebit;
  }
}
