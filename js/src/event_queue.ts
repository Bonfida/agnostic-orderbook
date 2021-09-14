import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserialize, deserializeUnchecked } from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";

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
  seqNum: BN;
  registerSize: number;

  static LEN: number = 37;

  static schema: Schema = new Map([
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
          ["register", "u32"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    tag: number;
    head: BN;
    count: BN;
    eventSize: BN;
    registerSize: number;
    seqNum: BN;
  }) {
    this.tag = arg.tag as AccountTag;
    this.head = arg.head;
    this.count = arg.count;
    this.eventSize = arg.eventSize;
    this.registerSize = arg.registerSize;
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

  static deserialize(callbackInfoLen: number, data: Buffer) {
    return new EventFill({
      takerSide: data.slice(1, 1).readInt8(),
      makerOrderId: new BN(data.slice(2, 18), "le"),
      quoteSize: new BN(data.slice(18, 26), "le"),
      assetSize: new BN(data.slice(26, 34), "le"),
      makerCallbackInfo: [...data.slice(34, 34 + callbackInfoLen)],
      takerCallbackInfo: [
        ...data.slice(34 + callbackInfoLen, 34 + 2 * callbackInfoLen),
      ],
    });
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

  static deserialize(callbackInfoLen: number, data: Buffer) {
    return new EventOut({
      side: data.slice(1, 1).readInt8(),
      orderId: new BN(data.slice(2, 18), "le"),
      assetSize: new BN(data.slice(18, 26), "le"),
      delete: data.slice(26).readUInt8(),
      callBackInfo: [...data.slice(27, 27 + callbackInfoLen)],
    });
  }
}

export class EventQueue {
  header: EventQueueHeader;
  buffer: number[];
  callBackInfoLen: number;

  // static LEN_FILL = 1 + 16 + 8 + 8 + 1 + 1;
  // static LEN_OUT = 1 + 16 + 8 + 8 + 1;

  constructor(arg: {
    header: EventQueueHeader;
    buffer: number[];
    callBackInfoLen: number;
  }) {
    this.header = arg.header;
    this.buffer = arg.buffer;
    this.callBackInfoLen = arg.callBackInfoLen;
  }

  static parse(callBackInfoLen: number, data: Buffer) {
    return new EventQueue({
      header: deserializeUnchecked(
        EventQueueHeader.schema,
        EventQueueHeader,
        data
      ) as EventQueueHeader,
      buffer: [...data],
      callBackInfoLen,
    });
  }

  static async load(
    connection: Connection,
    address: PublicKey,
    callBackInfoLen: number
  ) {
    const accountInfo = await connection.getAccountInfo(address);
    if (!accountInfo?.data) {
      throw new Error("Invalid address provided");
    }
    return this.parse(callBackInfoLen, accountInfo.data);
  }

  parseEvent(idx: number) {
    let offset =
      EventQueueHeader.LEN +
      this.header.registerSize +
      ((idx * this.header.eventSize.toNumber() + this.header.head.toNumber()) %
        this.buffer.length);
    let data = Buffer.from(this.buffer.slice(offset));
    switch (data[0]) {
      case EventType.Fill:
        return EventFill.deserialize(this.callBackInfoLen, data) as EventFill;
      case EventType.Out:
        return EventOut.deserialize(this.callBackInfoLen, data) as EventOut;
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
    return deserialize(
      EventQueueHeader.schema,
      EventQueueHeader,
      data
    ) as EventQueueHeader;
  }

  extractRegister(data: Buffer) {
    return data.slice(
      EventQueueHeader.LEN,
      EventQueueHeader.LEN + this.header.registerSize
    );
  }
}
