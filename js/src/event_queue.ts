import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserialize, deserializeUnchecked } from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";
import { BytesSlab } from "./slab";

///////////////////////////////////////////////
////// Event Queue
///////////////////////////////////////////////

export enum EventType {
  Fill = 0,
  Out = 1,
}

export class EventQueueHeader {
  tag: AccountTag;
  head: BN;
  count: BN;
  eventSize: BN;
  register: number[];
  seqNum: BN;

  constructor(arg: {
    tag: number;
    head: BN;
    count: BN;
    eventSize: BN;
    register: number[];
    seqNum: BN;
  }) {
    this.tag = arg.tag as AccountTag;
    this.head = arg.head;
    this.count = arg.count;
    this.eventSize = arg.eventSize;
    this.register = arg.register;
    this.seqNum = arg.seqNum;
  }
}

export enum Side {
  Bid = 0,
  Ask = 1,
}

export class EventFill {
  takerSide: Side;
  makerOrderId: BN;
  quoteSize: BN;
  assetSize: BN;
  makerCallbackInfo: number[];
  takerCallbackInfo: number[];

  constructor(arg: {
    takerSide: number;
    makerOrderId: BN;
    quoteSize: BN;
    assetSize: BN;
    makerCallbackInfo: number[];
    takerCallbackInfo: number[];
  }) {
    this.takerSide = arg.takerSide as Side;
    this.makerOrderId = arg.makerOrderId;
    this.quoteSize = arg.quoteSize;
    this.assetSize = arg.assetSize;
    this.makerCallbackInfo = arg.makerCallbackInfo;
    this.takerCallbackInfo = arg.takerCallbackInfo;
  }
}

export class EventOut {
  side: Side;
  orderId: BN;
  assetSize: BN;
  delete: boolean;
  callBackInfo: number[];

  constructor(arg: {
    side: number;
    orderId: BN;
    assetSize: BN;
    delete: number;
    callBackInfo: number[];
  }) {
    this.side = arg.side as Side;
    this.orderId = arg.orderId;
    this.assetSize = arg.assetSize;
    this.delete = arg.delete === 1;
    this.callBackInfo = arg.callBackInfo;
  }
}

export class EventQueue {
  header: EventQueueHeader;
  buffer: number[];
  callBackInfoLen: number;

  static LEN_FILL = 1 + 16 + 8 + 8 + 1 + 1;
  static LEN_OUT = 1 + 16 + 8 + 8 + 1;

  // @ts-ignore
  static schema: Schema = new Map([
    [
      EventQueue,
      {
        kind: "struct",
        fields: [
          ["header", EventQueueHeader],
          ["buffer", BytesSlab],
          ["callBackInfoLen", "u8"],
        ],
      },
    ],
    [
      EventQueueHeader,
      {
        kind: "struct",
        fields: [
          ["accountFlags", "u64"],
          ["head", "u64"],
          ["count", "u64"],
          ["eventSize", "u64"],
          ["seqNum", "u64"],
          ["register", ["u8"]],
        ],
      },
    ],
    [
      EventFill,
      {
        kind: "struct",
        fields: [
          ["side", "u8"],
          ["makerOrderId", "u128"],
          ["quoteSize", "u64"],
          ["assetSize", "u64"],
          ["makerCallbackInfo", ["u8"]],
          ["takerCallbackInfo", ["u8"]],
        ],
      },
    ],
    [
      EventOut,
      {
        kind: "struct",
        fields: [
          ["side", "u8"],
          ["orderId", "u128"],
          ["quoteSize", "u64"],
          ["assetSize", "u64"],
          ["delete", "u8"],
          ["callBackInfo", ["u8"]],
        ],
      },
    ],
  ]);

  constructor(arg: {
    header: EventQueueHeader;
    buffer: number[];
    callBackInfoLen: number;
  }) {
    this.header = arg.header;
    this.buffer = arg.buffer;
    this.callBackInfoLen = arg.callBackInfoLen;
  }

  static parse(data: Buffer) {
    return deserializeUnchecked(this.schema, EventQueue, data) as EventQueue;
  }

  static async load(connection: Connection, address: PublicKey) {
    const accountInfo = await connection.getAccountInfo(address);
    if (!accountInfo?.data) {
      throw new Error("Invalid address provided");
    }
    return this.parse(accountInfo.data);
  }

  parseEvent(idx: number) {
    let offset =
      (idx * this.header.eventSize.toNumber() + this.header.head.toNumber()) %
      this.buffer.length;
    let data = Buffer.from(this.buffer.slice(offset));
    switch (data[0]) {
      case EventType.Fill:
        return deserializeUnchecked(
          EventQueue.schema,
          EventFill,
          data
        ) as EventFill;
      case EventType.Out:
        return deserializeUnchecked(
          EventQueue.schema,
          EventOut,
          data
        ) as EventOut;
      default:
        throw new Error("Invalid data provided");
    }
  }

  parseFill(limit?: number) {
    const n = limit
      ? Math.min(limit, this.header.count.toNumber())
      : this.header.count.toNumber();
    return [...Array(n).keys()]
      .map((e) => this.parseEvent(e))
      .filter((e) => e instanceof EventFill);
  }

  static parseEventQueueHeader(data: Buffer) {
    return deserialize(this.schema, EventQueueHeader, data) as EventQueueHeader;
  }
}
