import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserialize, deserializeUnchecked } from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";

/** @enum {number} */
export enum EventType {
  Fill = 0,
  Out = 1,
}

/** @enum {number} */
export enum Side {
  Bid = 0,
  Ask = 1,
}

/**
 * Event queue header object
 */
export class EventQueueHeader {
  tag: AccountTag;
  head: BN;
  count: BN;
  eventSize: BN;
  seqNum: BN;

  static LEN: number = 37;
  static REGISTER_SIZE: number = 42;

  /**
   * @param callBackInfoLen number of bytes in the callback info
   * @returns event queue slot size
   */
  static computeSlotSize(callBackInfoLen: BN) {
    return callBackInfoLen.muln(2).addn(1 + 33);
  }

  static schema: Schema = new Map([
    [
      EventQueueHeader,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["head", "u64"],
          ["count", "u64"],
          ["eventSize", "u64"],
          ["seqNum", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    tag: number;
    head: BN;
    count: BN;
    eventSize: BN;
    seqNum: BN;
  }) {
    this.tag = arg.tag as AccountTag;
    this.head = arg.head;
    this.count = arg.count;
    this.eventSize = arg.eventSize;
    this.seqNum = arg.seqNum;
  }
}

/**
 * Event fill object
 */
export class EventFill {
  takerSide: Side;
  makerOrderId: BN;
  quoteSize: BN;
  baseSize: BN;
  makerCallbackInfo: number[];
  takerCallbackInfo: number[];

  constructor(arg: {
    takerSide: number;
    makerOrderId: BN;
    quoteSize: BN;
    baseSize: BN;
    makerCallbackInfo: number[];
    takerCallbackInfo: number[];
  }) {
    this.takerSide = arg.takerSide as Side;
    this.makerOrderId = arg.makerOrderId;
    this.quoteSize = arg.quoteSize;
    this.baseSize = arg.baseSize;
    this.makerCallbackInfo = arg.makerCallbackInfo;
    this.takerCallbackInfo = arg.takerCallbackInfo;
  }

  /**
   * Deserialize a buffer into an EventFill object
   * @param callbackInfoLen Length of the callback information
   * @param data Buffer to deserialize
   * @returns Returns an EventFill object
   */
  static deserialize(callbackInfoLen: number, data: Buffer) {
    return new EventFill({
      takerSide: data[1],
      makerOrderId: new BN(data.slice(2, 18), "le"),
      quoteSize: new BN(data.slice(18, 26), "le"),
      baseSize: new BN(data.slice(26, 34), "le"),
      makerCallbackInfo: [...data.slice(34, 34 + callbackInfoLen)],
      takerCallbackInfo: [
        ...data.slice(34 + callbackInfoLen, 34 + 2 * callbackInfoLen),
      ],
    });
  }
}

/**
 * EventOut object
 */
export class EventOut {
  side: Side;
  orderId: BN;
  baseSize: BN;
  delete: boolean;
  callBackInfo: number[];

  constructor(arg: {
    side: number;
    orderId: BN;
    baseSize: BN;
    delete: number;
    callBackInfo: number[];
  }) {
    this.side = arg.side as Side;
    this.orderId = arg.orderId;
    this.baseSize = arg.baseSize;
    this.delete = arg.delete === 1;
    this.callBackInfo = arg.callBackInfo;
  }

  /**
   * Deserialize a buffer into an EventOut object
   * @param callbackInfoLen Length of the callback information
   * @param data Buffer to deserialize
   * @returns Returns an EventOut object
   */
  static deserialize(callbackInfoLen: number, data: Buffer) {
    return new EventOut({
      side: data[1],
      orderId: new BN(data.slice(2, 18), "le"),
      baseSize: new BN(data.slice(18, 26), "le"),
      delete: data[26],
      callBackInfo: [...data.slice(27, 27 + callbackInfoLen)],
    });
  }
}

/**
 * Event queue object
 */
export class EventQueue {
  header: EventQueueHeader;
  buffer: number[];
  callBackInfoLen: number;

  constructor(arg: {
    header: EventQueueHeader;
    buffer: number[];
    callBackInfoLen: number;
  }) {
    this.header = arg.header;
    this.buffer = arg.buffer;
    this.callBackInfoLen = arg.callBackInfoLen;
  }

  /**
   * Deserialize a buffer into an EventQueue object
   * @param callBackInfoLen Length of the callback information
   * @param data Buffer to deserialize
   * @returns Returns an EventQueue object
   */
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

  /**
   * Loads the event queue from its address
   * @param connection The solana connection object to the RPC node
   * @param address The address of the event queue
   * @param callBackInfoLen The length of the callback information
   * @returns Returns an EventQueue object
   */
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

  /**
   * Returns an event from its index in the event queue
   * @param idx Index of the event to parse
   * @returns Returns an Event object
   */
  parseEvent(idx: number) {
    let header_offset = EventQueueHeader.LEN + EventQueueHeader.REGISTER_SIZE;
    let offset =
      header_offset +
      ((idx * this.header.eventSize.toNumber() + this.header.head.toNumber()) %
        (this.buffer.length - header_offset));
    let data = Buffer.from(
      this.buffer.slice(offset, offset + this.header.eventSize.toNumber())
    );
    switch (data[0]) {
      case EventType.Fill:
        return EventFill.deserialize(this.callBackInfoLen, data) as EventFill;
      case EventType.Out:
        return EventOut.deserialize(this.callBackInfoLen, data) as EventOut;
      default:
        throw new Error("Invalid data provided");
    }
  }

  /**
   * Returns fill events from the event queue
   * @param limit Optional limit parameter
   * @returns An array of EventFill
   */
  parseFill(limit?: number) {
    const n = limit
      ? Math.min(limit, this.header.count.toNumber())
      : this.header.count.toNumber();
    return [...Array(n).keys()]
      .map((e) => this.parseEvent(e))
      .filter((e) => e instanceof EventFill);
  }

  /**
   * Deserialize a buffer into an EventQueueHeader object
   * @param data Buffer to deserialize
   * @returns Returns an EventQueueHeader object
   */
  static parseEventQueueHeader(data: Buffer) {
    return deserialize(
      EventQueueHeader.schema,
      EventQueueHeader,
      data
    ) as EventQueueHeader;
  }

  /**
   * Extract the event queue registrar
   * @param data Buffer to extract the registrar from
   * @returns Returns the event queue registrar data as a buffer
   */
  extractRegister(data: Buffer) {
    return data.slice(
      EventQueueHeader.LEN,
      EventQueueHeader.LEN + EventQueueHeader.REGISTER_SIZE
    );
  }
}
