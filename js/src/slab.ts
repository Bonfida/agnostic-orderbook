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

  static LEN = 32;

  static schema: Schema = new Map([
    [
      InnerNode,
      {
        kind: "struct",
        fields: [
          ["key", "u128"],
          ["prefixLen", "u64"],
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
  baseQuantity: BN;

  static LEN = 24;

  static schema: Schema = new Map([
    [
      LeafNode,
      {
        kind: "struct",
        fields: [
          ["key", "u128"],
          ["baseQuantity", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: { key: BN; baseQuantity: BN }) {
    this.key = arg.key;
    this.baseQuantity = arg.baseQuantity;
  }

  /**
   * @return the price of this order
   */
  getPrice(): BN {
    return this.key.shrn(64);
  }
}

function isLeaf(handle: number): boolean {
  return (handle & Slab.INNER_FLAG) == 0;
}

export class SlabHeader {
  accountTag: AccountTag;
  leafFreeListLen: number;
  leafFreeListHead: number;
  leafBumpIndex: number;
  innerNodeFreeListLen: number;
  innerNodeFreeListHead: number;
  innerNodeBumpIndex: number;
  rootNode: number;
  leafCount: number;

  static LEN: number = 40;

  static schema: Schema = new Map([
    [
      SlabHeader,
      {
        kind: "struct",
        fields: [
          ["accountTag", "u64"],

          ["leafFreeListLen", "u32"],
          ["leafFreeListHead", "u32"],
          ["leafBumpIndex", "u32"],

          ["innerNodeFreeListLen", "u32"],
          ["innerNodeFreeListHead", "u32"],
          ["innerNodeBumpIndex", "u32"],

          ["rootNode", "u32"],
          ["leafCount", "u32"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    accountTag: BN;
    leafFreeListLen: number;
    leafFreeListHead: number;
    leafBumpIndex: number;
    innerNodeFreeListLen: number;
    innerNodeFreeListHead: number;
    innerNodeBumpIndex: number;
    rootNode: number;
    leafCount: number;
  }) {
    this.accountTag = arg.accountTag.toNumber() as AccountTag;
    this.rootNode = arg.rootNode;
    this.leafCount = arg.leafCount;

    this.leafFreeListLen = arg.leafFreeListLen;
    this.leafFreeListHead = arg.leafFreeListHead;
    this.leafBumpIndex = arg.leafBumpIndex;
    this.innerNodeFreeListLen = arg.innerNodeFreeListLen;
    this.innerNodeFreeListHead = arg.innerNodeFreeListHead;
    this.innerNodeBumpIndex = arg.innerNodeBumpIndex;
  }
}

export interface LeafNodeRef {
  leafNode: LeafNode;
  callbackInfo: Buffer;
}

export class Slab {
  header: SlabHeader;
  leafBuffer: Buffer;
  innerNodeBuffer: Buffer;
  callbackInfoBuffer: Buffer;
  callBackInfoLen: number;
  orderCapacity: number;

  static NODE_SIZE: number = 32;
  static NODE_TAG_SIZE: number = 8;
  static SLOT_SIZE: number = Slab.NODE_TAG_SIZE + Slab.NODE_SIZE;
  static INNER_FLAG: number = 1 << 31;

  constructor(arg: {
    header: SlabHeader;
    buffer: Buffer;
    callBackInfoLen: number;
  }) {
    this.header = arg.header;
    this.callBackInfoLen = arg.callBackInfoLen;
    const leafSize = LeafNode.LEN + arg.callBackInfoLen;

    const capacity =
      (arg.buffer.length - SlabHeader.LEN - leafSize) /
      (leafSize + InnerNode.LEN);
    let innerNodesBufferOffset = SlabHeader.LEN + (capacity + 1) * LeafNode.LEN;
    let leavesBuffer = arg.buffer.slice(SlabHeader.LEN, innerNodesBufferOffset);
    let callbackInfoBufferOffset =
      innerNodesBufferOffset + capacity * InnerNode.LEN;
    let innerNodeBuffer = arg.buffer.slice(
      innerNodesBufferOffset,
      callbackInfoBufferOffset
    );
    let callbackInfoBuffer = arg.buffer.slice(callbackInfoBufferOffset);
    this.orderCapacity = Math.floor(capacity);
    this.leafBuffer = leavesBuffer;
    this.innerNodeBuffer = innerNodeBuffer;
    this.callbackInfoBuffer = callbackInfoBuffer;
  }

  static deserialize(data: Buffer, callBackInfoLen: number) {
    return new Slab({
      header: deserializeUnchecked(SlabHeader.schema, SlabHeader, data),
      buffer: data,
      callBackInfoLen,
    });
  }

  static computeAllocationSize(
    desiredOrderCapacity: number,
    callbackInfoLen: number
  ): number {
    return (
      SlabHeader.LEN +
      LeafNode.LEN +
      callbackInfoLen +
      (desiredOrderCapacity - 1) *
        (LeafNode.LEN + InnerNode.LEN + callbackInfoLen)
    );
  }

  /**
   * Returns a node by its key
   * @param key Key of the node to fetch
   * @returns A node LeafNode object
   */
  getNodeByKey(key: BN): LeafNode | undefined {
    if (this.header.leafCount == 0) {
      return undefined;
    }
    let nodeHandle = this.header.rootNode;
    while (true) {
      let node = this.getNode(nodeHandle);
      if (node instanceof InnerNode) {
        let common_prefix_len = 128 - node.key.xor(key).bitLength();
        if (common_prefix_len < node.prefixLen.toNumber()) {
          return undefined;
        }
        const critBitMasks = new BN(1).shln(127 - node.prefixLen.toNumber());
        let critBit = key.and(critBitMasks).isZero() ? 0 : 1;
        nodeHandle = node.children[critBit];
      } else if (node instanceof LeafNode) {
        if (node.key.cmp(key) !== 0) {
          return undefined;
        }
        return node;
      } else {
        throw new Error("Couldn't parse node!");
      }
    }
  }

  getNode(handle: number): InnerNode | LeafNode {
    if (isLeaf(handle)) {
      let buff = this.leafBuffer.slice(
        handle * LeafNode.LEN,
        (handle + 1) * LeafNode.LEN
      );
      return deserializeUnchecked(LeafNode.schema, LeafNode, buff) as LeafNode;
    }
    let index = new BN(handle).notn(32).toNumber();
    let buff = this.innerNodeBuffer.slice(
      index * InnerNode.LEN,
      (index + 1) * InnerNode.LEN
    );
    return deserializeUnchecked(InnerNode.schema, InnerNode, buff) as InnerNode;
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
  *items(descending = false): Generator<LeafNodeRef> {
    if (this.header.leafCount == 0) {
      return;
    }
    const stack = [this.header.rootNode];
    while (stack.length > 0) {
      const nodeHandle = stack.pop() as number;
      const node = this.getNode(nodeHandle);
      if (node instanceof LeafNode) {
        yield {
          leafNode: node,
          callbackInfo: this.getCallBackInfo(nodeHandle) as Buffer,
        };
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
    const minMaxOrders: LeafNodeRef[] = [];
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
    if (this.header.leafCount == 0) {
      return [];
    }
    let raw: BN[] = [];
    let stack = [this.header.rootNode];
    while (true) {
      const current = stack.pop();
      if (current === undefined) break;
      const node = this.getNode(current);
      if (node instanceof LeafNode) {
        const leafPrice = node.getPrice();
        if (raw[raw.length - 1]?.eq(leafPrice)) {
          const idx = raw.length - 2;
          raw[idx].iadd(node.baseQuantity);
        } else if (raw.length === 2 * depth) {
          // The price has changed and we have enough prices. Note that the
          // above branch will be hit even if we already have `depth` prices
          // so that we will finish accumulating the current level. For example,
          // if we request one level and there are two order at the best price,
          // we will accumulate both orders.
          break;
        } else {
          raw.push(node.baseQuantity);
          raw.push(leafPrice);
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
        size: raw[2 * i],
        price: raw[2 * i + 1],
      });
    }
    return result;
  }

  /**
   * @param callBackInfoPt a leaf node's callBackInfoPt that gives the offset to
   * the info in the appropriate Slab.
   * @returns the raw binary callback info for the node
   */
  getCallBackInfo(nodeHandle: number) {
    if (!isLeaf(nodeHandle)) {
      return undefined;
    }
    return this.callbackInfoBuffer.slice(
      nodeHandle * this.callBackInfoLen,
      (nodeHandle + 1) * this.callBackInfoLen
    );
  }
}
