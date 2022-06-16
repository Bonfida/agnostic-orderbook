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
  seqNum: BN;

  static LEN: number = 32;

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
          ["tag", "u64"],
          ["head", "u64"],
          ["count", "u64"],
          ["seqNum", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: { tag: number; head: BN; count: BN; seqNum: BN }) {
    this.tag = arg.tag as AccountTag;
    this.head = arg.head;
    this.count = arg.count;
    this.seqNum = arg.seqNum;
  }
}

/**
 * Event fill object
 */
export class EventFill {
  takerSide: Side;
  quoteSize: BN;
  makerOrderId: BN;
  baseSize: BN;
  makerCallbackInfo!: number[];
  takerCallbackInfo!: number[];

  static LEN: number = 40;

  static schema: Schema = new Map([
    [
      EventFill,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["takerSide", "u8"],
          ["_padding", [6]],
          ["quoteSize", "u64"],
          ["makerOrderId", "u128"],
          ["baseSize", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    tag: number;
    takerSide: Side;
    quoteSize: BN;
    makerOrderId: BN;
    baseSize: BN;
  }) {
    this.takerSide = arg.takerSide as Side;
    this.makerOrderId = arg.makerOrderId;
    this.quoteSize = arg.quoteSize;
    this.baseSize = arg.baseSize;
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
  callbackInfo!: number[];

  static schema: Schema = new Map([
    [
      EventOut,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["side", "u8"],
          ["delete", "u8"],
          ["_padding", [13]],
          ["orderId", "u128"],
          ["baseSize", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    side: number;
    orderId: BN;
    baseSize: BN;
    delete: number;
  }) {
    this.side = arg.side as Side;
    this.orderId = arg.orderId;
    this.baseSize = arg.baseSize;
    this.delete = arg.delete === 1;
  }
}

/**
 * Event queue object
 */
export class EventQueue {
  header: EventQueueHeader;
  eventsBuffer: number[];
  callbackInfosBuffer: number[];
  callBackInfoLen: number;

  constructor(arg: {
    header: EventQueueHeader;
    eventsBuffer: number[];
    callbackInfosBuffer: number[];
    callBackInfoLen: number;
  }) {
    this.header = arg.header;
    this.eventsBuffer = arg.eventsBuffer;
    this.callbackInfosBuffer = arg.callbackInfosBuffer;
    this.callBackInfoLen = arg.callBackInfoLen;
  }

  /**
   * Deserialize a buffer into an EventQueue object
   * @param callBackInfoLen Length of the callback information
   * @param data Buffer to deserialize
   * @returns Returns an EventQueue object
   */
  static parse(callBackInfoLen: number, data: Buffer) {
    let header = deserializeUnchecked(
      EventQueueHeader.schema,
      EventQueueHeader,
      data
    ) as EventQueueHeader;
    let capacity =
      (data.length - EventQueueHeader.LEN) /
      (EventFill.LEN + 2 * callBackInfoLen);
    let callbackInfosOffset = capacity * EventFill.LEN;
    let eventsBuffer = data.slice(EventQueueHeader.LEN, callbackInfosOffset);
    let callbackInfosBuffer = data.slice(callbackInfosOffset);
    return new EventQueue({
      header,
      eventsBuffer: [...eventsBuffer],
      callbackInfosBuffer: [...callbackInfosBuffer],
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
    let eventsOffset = idx * EventFill.LEN;
    let data = Buffer.from(
      this.eventsBuffer.slice(eventsOffset, eventsOffset + EventFill.LEN)
    );
    switch (data[0]) {
      case EventType.Fill: {
        let event = deserializeUnchecked(
          EventFill.schema,
          EventFill,
          data
        ) as EventFill;
        let makerOffset = 2 * idx * this.callBackInfoLen;
        let takerOffset = (2 * idx + 1) * this.callBackInfoLen;
        event.makerCallbackInfo = this.callbackInfosBuffer.slice(
          makerOffset,
          makerOffset + this.callBackInfoLen
        );
        event.takerCallbackInfo = this.callbackInfosBuffer.slice(
          takerOffset,
          takerOffset + this.callBackInfoLen
        );
        return event;
      }
      case EventType.Out: {
        let event = deserializeUnchecked(
          EventOut.schema,
          EventOut,
          data
        ) as EventOut;
        let offset = 2 * idx * this.callBackInfoLen;
        event.callbackInfo = this.callbackInfosBuffer.slice(
          offset,
          offset + this.callBackInfoLen
        );
        return event;
      }
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

  static computeAllocationSize(
    desiredEventCapacity: number,
    callbackInfoLen: number
  ): number {
    return (
      desiredEventCapacity * (EventFill.LEN + 2 * callbackInfoLen) +
      EventQueueHeader.LEN
    );
  }
}
