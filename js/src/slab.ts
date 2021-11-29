import { PublicKey } from "@solana/web3.js";
import { Schema, deserializeUnchecked } from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";
import { Price } from "./types";
// Uncomment to use WebAssembly for OB deserialization
// import { find_max, find_min, find_l2_depth } from "dex-wasm";

///////////////////////////////////////////////
////// Nodes and Slab
///////////////////////////////////////////////

export class InnerNode {
  prefixLen: BN;
  key: BN;
  children: number[];

  static schema: Schema = new Map([
    [
      InnerNode,
      {
        kind: "struct",
        fields: [
          ["prefixLen", "u64"],
          ["key", "u128"],
          ["children", ["u32", 2]],
        ],
      },
    ],
  ]);

  constructor(arg: { prefixLen: BN; key: BN; children: number[] }) {
    this.prefixLen = arg.prefixLen;
    this.key = arg.key;
    this.children = arg.children;
  }
}

export class LeafNode {
  key: BN;
  callBackInfoPt: BN;
  baseQuantity: BN;

  static schema: Schema = new Map([
    [
      LeafNode,
      {
        kind: "struct",
        fields: [
          ["key", "u128"],
          ["callBackInfoPt", "u64"],
          ["baseQuantity", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: { key: BN; callBackInfoPt: BN; baseQuantity: BN }) {
    this.key = arg.key;
    this.callBackInfoPt = arg.callBackInfoPt;
    this.baseQuantity = arg.baseQuantity;
  }

  /**
   * @return the price of this order
   */
  getPrice(): BN {
    return this.key.shrn(64);
  }
}

export class FreeNode {
  next: number;

  static schema: Schema = new Map([
    [
      FreeNode,
      {
        kind: "struct",
        fields: [["next", "u32"]],
      },
    ],
  ]);

  constructor(arg: { next: number }) {
    this.next = arg.next;
  }
}

/**
 * Deserializes a node buffer
 * @param data Buffer to deserialize
 * @returns Returns a node
 */
export function parseNode(
  data: Buffer
): undefined | FreeNode | LeafNode | InnerNode {
  switch (data[0]) {
    case 0:
      throw new Error("node is unitialized");
    case 1:
      return deserializeUnchecked(
        InnerNode.schema,
        InnerNode,
        data.slice(Slab.NODE_TAG_SIZE)
      );
    case 2:
      return deserializeUnchecked(
        LeafNode.schema,
        LeafNode,
        data.slice(Slab.NODE_TAG_SIZE)
      );
    case 3:
      return deserializeUnchecked(
        FreeNode.schema,
        FreeNode,
        data.slice(Slab.NODE_TAG_SIZE)
      );
    case 4:
      return deserializeUnchecked(
        FreeNode.schema,
        FreeNode,
        data.slice(Slab.NODE_TAG_SIZE)
      );
    default:
      throw new Error("Invalid data");
  }
}

export class SlabHeader {
  accountTag: AccountTag;
  bumpIndex: BN;
  freeListLen: BN;
  freeListHead: number;
  callbackMemoryOffset: BN;
  callbackFreeListLen: BN;
  callbackFreeListHead: BN;
  callbackBumpIndex: BN;
  rootNode: number;
  leafCount: BN;
  marketAddress: PublicKey;

  static LEN: number = 97;
  static PADDED_LEN: number = SlabHeader.LEN + 7;

  static schema: Schema = new Map([
    [
      SlabHeader,
      {
        kind: "struct",
        fields: [
          ["accountTag", "u8"],
          ["bumpIndex", "u64"],
          ["freeListLen", "u64"],
          ["freeListHead", "u32"],
          ["callbackMemoryOffset", "u64"],
          ["callbackFreeListLen", "u64"],
          ["callbackFreeListHead", "u64"],
          ["callbackBumpIndex", "u64"],
          ["rootNode", "u32"],
          ["leafCount", "u64"],
          ["marketAddress", [32]],
        ],
      },
    ],
  ]);

  constructor(arg: {
    accountTag: number;
    bumpIndex: BN;
    freeListLen: BN;
    freeListHead: number;
    callbackMemoryOffset: BN;
    callbackFreeListLen: BN;
    callbackFreeListHead: BN;
    callbackBumpIndex: BN;
    rootNode: number;
    leafCount: BN;
    marketAddress: Uint8Array;
  }) {
    this.accountTag = arg.accountTag as AccountTag;
    this.bumpIndex = arg.bumpIndex;
    this.freeListLen = arg.freeListLen;
    this.freeListHead = arg.freeListHead;
    this.callbackMemoryOffset = arg.callbackMemoryOffset;
    this.callbackFreeListLen = arg.callbackFreeListLen;
    this.callbackFreeListHead = arg.callbackFreeListHead;
    this.callbackBumpIndex = arg.callbackBumpIndex;
    this.rootNode = arg.rootNode;
    this.leafCount = arg.leafCount;
    this.marketAddress = new PublicKey(arg.marketAddress);
  }
}

export class Slab {
  header: SlabHeader;
  buffer: Buffer;
  callBackInfoLen: BN;
  orderCapacity: number;
  callbackMemoryOffset: BN;

  static NODE_SIZE: number = 32;
  static NODE_TAG_SIZE: number = 8;
  static SLOT_SIZE: number = Slab.NODE_TAG_SIZE + Slab.NODE_SIZE;

  constructor(arg: {
    header: SlabHeader;
    buffer: Buffer;
    callBackInfoLen: BN;
  }) {
    this.header = arg.header;
    this.buffer = arg.buffer;
    this.callBackInfoLen = arg.callBackInfoLen;

    const capacity = new BN(this.buffer.length - SlabHeader.PADDED_LEN);
    const size = this.callBackInfoLen.addn(Slab.SLOT_SIZE * 2);
    this.orderCapacity = Math.floor(capacity.div(size).toNumber());
    this.callbackMemoryOffset = new BN(this.orderCapacity)
      .muln(2 * Slab.SLOT_SIZE)
      .addn(SlabHeader.PADDED_LEN);
  }

  static deserialize(data: Buffer, callBackInfoLen: BN) {
    return new Slab({
      header: deserializeUnchecked(SlabHeader.schema, SlabHeader, data),
      buffer: data,
      callBackInfoLen,
    });
  }

  /**
   * Returns a node by its key
   * @param key Key of the node to fetch
   * @returns A node LeafNode object
   */
  getNodeByKey(key: number) {
    let pointer = this.header.rootNode;
    while (true) {
      const offset = SlabHeader.PADDED_LEN + pointer * Slab.SLOT_SIZE;
      let node = parseNode(this.buffer.slice(offset, offset + Slab.SLOT_SIZE));
      if (node instanceof InnerNode) {
        const critBitMaks = (1 << 127) >> node.prefixLen.toNumber();
        let critBit = key & critBitMaks;
        pointer = node.children[critBit];
      }
      if (node instanceof LeafNode) {
        return node;
      }
    }
  }

  // Uncomment if you are using webassembly
  // /**
  //  * Return min or max node of the critbit tree
  //  * @param max Boolean (false for best asks and true for best bids)
  //  * @returns Returns the min or max node of the Slab
  //  */
  // getMinMax(max: boolean) {
  //   let pointer;
  //   if (max) {
  //     pointer = find_max(
  //       this.data,
  //       BigInt(this.callBackInfoLen),
  //       BigInt(this.slotSize)
  //     );
  //   } else {
  //     pointer = find_min(
  //       this.data,
  //       BigInt(this.callBackInfoLen),
  //       BigInt(this.slotSize)
  //     );
  //   }
  //   let offset = SlabHeader.LEN;
  //   if (!pointer) {
  //     throw new Error("Empty slab");
  //   }
  //   let node = parseNode(
  //     this.callBackInfoLen,
  //     this.data.slice(
  //       offset + pointer * this.slotSize,
  //       offset + (pointer + 1) * this.slotSize
  //     )
  //   );
  //   return node;
  // }

  /**
   * Walkdown the critbit tree
   * @param descending
   * @returns
   */
  *items(descending = false): Generator<LeafNode> {
    if (this.header.leafCount.eq(new BN(0))) {
      return;
    }
    const stack = [this.header.rootNode];
    while (stack.length > 0) {
      const pointer = stack.pop();
      if (pointer === undefined) throw new Error("unreachable!");
      let offset = SlabHeader.PADDED_LEN + pointer * Slab.SLOT_SIZE;
      const node = parseNode(
        this.buffer.slice(offset, offset + Slab.SLOT_SIZE)
      );
      if (node instanceof LeafNode) {
        yield node;
      } else if (node instanceof InnerNode) {
        if (descending) {
          stack.push(node.children[0], node.children[1]);
        } else {
          stack.push(node.children[1], node.children[0]);
        }
      }
    }
  }

  [Symbol.iterator]() {
    return this.items(false);
  }

  // Uncomment if you are using webassembly
  // /**
  //  * Returns an array of [price, size] given a certain depth
  //  * @param depth Depth to fetch
  //  * @param max Boolean (false for asks and true for bids)
  //  * @returns Returns an array made of [price, size] elements
  //  */
  // getL2Depth(depth: number, increasing: boolean): Price[] {
  //   let raw = find_l2_depth(
  //     this.data,
  //     BigInt(this.callBackInfoLen),
  //     BigInt(this.slotSize),
  //     BigInt(depth),
  //     increasing
  //   );
  //   let result: Price[] = [];
  //   for (let i = 0; i < raw.length / 2; i++) {
  //     result.push({
  //       size: Number(raw[2 * i]),
  //       price: Number(raw[2 * i + 1]) / 2 ** 32,
  //     });
  //   }
  //   return result;
  // }

  /**
   * Returns the top maxNbOrders (not aggregated by price)
   * @param maxNbOrders
   * @param max Boolean (false for asks and true for bids)
   * @returns Returns an array of LeafNode object
   */
  getMinMaxNodes(maxNbOrders: number, max: boolean) {
    const minMaxOrders: LeafNode[] = [];
    for (const leafNode of this.items(max)) {
      if (minMaxOrders.length === maxNbOrders) {
        break;
      }
      minMaxOrders.push(leafNode);
    }
    return minMaxOrders;
  }

  /**
   * Aggregates price levels up to the given depth
   * @param depth maximum number of price levels
   * @param increasing true to return in increasing order
   * @returns aggregated quantities at each price level
   */
  getL2DepthJS(depth: number, increasing: boolean): Price[] {
    if (this.header.leafCount.eq(new BN(0))) {
      return [];
    }
    let raw: number[] = [];
    let stack = [this.header.rootNode];
    while (true) {
      const current = stack.pop();
      if (current === undefined) break;
      let offset = SlabHeader.PADDED_LEN + current * Slab.SLOT_SIZE;
      const node = parseNode(
        this.buffer.slice(offset, offset + Slab.SLOT_SIZE)
      );
      if (node instanceof LeafNode) {
        const leafPrice = node.getPrice();
        if (raw[raw.length - 1] === leafPrice.toNumber()) {
          const idx = raw.length - 2;
          raw[idx] += node.baseQuantity.toNumber();
        } else if (raw.length === 2 * depth) {
          // The price has changed and we have enough prices. Note that the
          // above branch will be hit even if we already have `depth` prices
          // so that we will finish accumulating the current level. For example,
          // if we request one level and there are two order at the best price,
          // we will accumulate both orders.
          break;
        } else {
          raw.push(node.baseQuantity.toNumber());
          raw.push(leafPrice.toNumber());
        }
      }
      if (node instanceof InnerNode) {
        stack.push(node.children[increasing ? 1 : 0]);
        stack.push(node.children[increasing ? 0 : 1]);
      }
    }
    let result: Price[] = [];
    for (let i = 0; i < raw.length / 2; i++) {
      result.push({
        size: Number(raw[2 * i]),
        price: Number(raw[2 * i + 1]),
      });
    }
    return result;
  }

  /**
   * @param callBackInfoPt a leaf node's callBackInfoPt that gives the offset to
   * the info in the appropriate Slab.
   * @returns the raw binary callback info for the node
   */
  getCallBackInfo(callBackInfoPt: BN) {
    return this.buffer.slice(
      callBackInfoPt.toNumber(),
      callBackInfoPt.add(this.callBackInfoLen).toNumber()
    );
  }
}
