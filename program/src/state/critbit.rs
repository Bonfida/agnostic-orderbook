#![allow(missing_docs)]
use crate::error::AoError;
use crate::state::AccountTag;
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;
// A Slab contains the data for a slab header and an array of nodes of a critbit tree
// whose leafs contain the data referencing an order of the orderbook.

#[doc(hidden)]
pub type IoError = std::io::Error;

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct SlabHeader {
    callback_free_list_len: u64,
    callback_free_list_head: u64,

    leaf_free_list_len: u32,
    leaf_free_list_head: u32,
    leaf_bump_index: u32,

    inner_node_free_list_len: u32,
    inner_node_free_list_head: u32,
    inner_node_bump_index: u32,

    root_node: u32,
    pub leaf_count: u32,
    market_address: [u8; 32],
}

impl SlabHeader {
    pub const LEN: usize = std::mem::size_of::<Self>();
}

pub struct Slab<'a, C> {
    pub header: &'a mut SlabHeader,
    pub leaf_nodes: &'a mut [LeafNode],
    pub inner_nodes: &'a mut [InnerNode],
    pub callback_infos: &'a mut [C],
}
#[derive(Zeroable, Clone, Copy, Pod, Debug, PartialEq)]
#[repr(C)]
pub struct LeafNode {
    /// The key is the associated order id
    pub key: u128,
    /// The quantity of base asset associated with the underlying order
    pub base_quantity: u64,
}

impl LeafNode {
    pub const LEN: usize = std::mem::size_of::<Self>();

    /// Parse a leaf node's price
    pub fn price(&self) -> u64 {
        Self::price_from_key(self.key)
    }

    /// Get the associated order id
    pub fn order_id(&self) -> u128 {
        self.key
    }

    #[inline(always)]
    pub(crate) fn price_from_key(key: u128) -> u64 {
        (key >> 64) as u64
    }
}

pub type NodeHandle = u32;

pub const INNER_FLAG: u32 = 1 << 31;
#[derive(Zeroable, Clone, Copy, Pod, Debug)]
#[repr(C)]
pub struct InnerNode {
    key: u128,
    prefix_len: u64,
    pub children: [u32; 2],
}

impl InnerNode {
    pub const LEN: usize = std::mem::size_of::<Self>();

    pub(crate) fn walk_down(&self, search_key: u128) -> (NodeHandle, bool) {
        let crit_bit_mask = (1u128 << 127) >> self.prefix_len;
        let crit_bit = (search_key & crit_bit_mask) != 0;
        (self.children[crit_bit as usize], crit_bit)
    }
}

pub enum Node {
    Leaf,
    Inner,
}

impl Node {
    pub fn from_handle(h: NodeHandle) -> Self {
        if h & INNER_FLAG == 0 {
            Self::Leaf
        } else {
            Self::Inner
        }
    }
}

impl<'slab, C> Slab<'slab, C> {
    pub fn initialize(
        asks_data: &mut [u8],
        bids_data: &mut [u8],
        market_address: Pubkey,
    ) -> Result<(), ProgramError> {
        if asks_data[0] != AccountTag::Uninitialized as u8
            || bids_data[0] != AccountTag::Uninitialized as u8
        {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        asks_data[0] = AccountTag::Asks as u8;
        let asks_header =
            bytemuck::from_bytes_mut::<SlabHeader>(&mut asks_data[8..8 + SlabHeader::LEN]);
        asks_header.market_address = market_address.to_bytes();

        bids_data[0] = AccountTag::Bids as u8;
        let bids_header =
            bytemuck::from_bytes_mut::<SlabHeader>(&mut bids_data[8..8 + SlabHeader::LEN]);
        bids_header.market_address = market_address.to_bytes();
        Ok(())
    }

    pub fn compute_allocation_size(desired_order_capacity: usize) -> usize {
        8 + SlabHeader::LEN
            + LeafNode::LEN
            + std::mem::size_of::<C>()
            + (desired_order_capacity.checked_sub(1).unwrap())
                * (LeafNode::LEN + InnerNode::LEN + std::mem::size_of::<C>())
    }
}

impl<'a, C: Pod> Slab<'a, C> {
    pub fn from_buffer(buf: &'a mut [u8], expected_tag: AccountTag) -> Result<Self, ProgramError> {
        let callback_info_len = std::mem::size_of::<C>();
        let leaf_size = LeafNode::LEN + callback_info_len;
        let capacity = (buf.len() - SlabHeader::LEN - 8 - leaf_size) / (leaf_size + InnerNode::LEN);

        if buf[0] != expected_tag as u8 {
            return Err(ProgramError::InvalidAccountData);
        }
        let (_, rem) = buf.split_at_mut(8);
        let (header, rem) = rem.split_at_mut(SlabHeader::LEN);
        let (leaves, rem) = rem.split_at_mut((capacity + 1) * LeafNode::LEN);
        let (inner_nodes, callback_infos) = rem.split_at_mut(capacity * InnerNode::LEN);
        let header = bytemuck::from_bytes_mut::<SlabHeader>(header);

        Ok(Self {
            header,
            leaf_nodes: bytemuck::cast_slice_mut::<_, LeafNode>(leaves),
            inner_nodes: bytemuck::cast_slice_mut::<_, InnerNode>(inner_nodes),
            callback_infos: bytemuck::cast_slice_mut::<_, C>(callback_infos),
        })
    }
}

impl<'a, C> Slab<'a, C> {
    pub fn root(&self) -> Option<NodeHandle> {
        if self.header.leaf_count == 0 {
            None
        } else {
            Some(self.header.root_node)
        }
    }
    pub(crate) fn allocate_leaf(&mut self) -> Result<NodeHandle, IoError> {
        if self.header.leaf_free_list_len == 0 {
            if self.header.leaf_bump_index as usize >= self.leaf_nodes.len() {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            let key = self.header.leaf_bump_index;
            self.header.leaf_bump_index += 1;
            return Ok(key);
        }

        let key = self.header.leaf_free_list_head;
        let free_leaf = &mut self.leaf_nodes[key as usize];
        let next = free_leaf.base_quantity as u32;
        self.header.leaf_free_list_head = next;
        self.header.leaf_free_list_len -= 1;

        Ok(key)
    }

    pub(crate) fn free_leaf(&mut self, handle: NodeHandle) {
        if self.header.leaf_free_list_len != 0 {
            let next = self.header.leaf_free_list_head;
            self.leaf_nodes[handle as usize].base_quantity = next as u64;
        }

        self.header.leaf_free_list_len += 1;
        self.header.leaf_free_list_head = handle;
    }

    pub(crate) fn allocate_inner_node(&mut self) -> Result<NodeHandle, IoError> {
        if self.header.inner_node_free_list_len == 0 {
            if self.header.inner_node_bump_index as usize >= self.inner_nodes.len() {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            let key = self.header.inner_node_bump_index;
            self.header.inner_node_bump_index += 1;
            return Ok(!key);
        }

        let key = self.header.inner_node_free_list_head;
        let free_inner_node = &mut self.inner_nodes[key as usize];
        let next = free_inner_node.prefix_len as u32;
        self.header.inner_node_free_list_head = next;
        self.header.inner_node_free_list_len -= 1;

        Ok(!key)
    }

    pub(crate) fn free_inner_node(&mut self, handle: NodeHandle) {
        if self.header.inner_node_free_list_len != 0 {
            let next = self.header.inner_node_free_list_head;
            self.inner_nodes[(!handle) as usize].prefix_len = next as u64;
        }

        self.header.inner_node_free_list_len += 1;
        self.header.inner_node_free_list_head = !handle;
    }

    pub(crate) fn insert_leaf(
        &mut self,
        new_leaf: &LeafNode,
    ) -> Result<(NodeHandle, Option<LeafNode>), AoError> {
        let mut root: NodeHandle = if self.header.leaf_count == 0 {
            // create a new root if none exists
            let new_leaf_handle = self.allocate_leaf().map_err(|_| AoError::SlabOutOfSpace)?;
            self.leaf_nodes[new_leaf_handle as usize] = *new_leaf;
            self.header.root_node = new_leaf_handle;
            self.header.leaf_count += 1;
            return Ok((new_leaf_handle, None));
        } else {
            self.header.root_node
        };
        let mut parent_node: Option<NodeHandle> = None;
        let mut previous_critbit: Option<bool> = None;
        loop {
            let shared_prefix_len = match Node::from_handle(root) {
                Node::Inner => {
                    let root_node = &self.inner_nodes[(!root) as usize];
                    let shared_prefix_len: u32 = (root_node.key ^ new_leaf.key).leading_zeros();
                    let keep_old_root = shared_prefix_len >= root_node.prefix_len as u32;
                    if keep_old_root {
                        parent_node = Some(root);
                        let r = root_node.walk_down(new_leaf.key);
                        root = r.0;
                        previous_critbit = Some(r.1);
                        continue;
                    }

                    shared_prefix_len
                }
                Node::Leaf => {
                    let root_node = &mut self.leaf_nodes[root as usize];
                    if root_node.key == new_leaf.key {
                        // clobber the existing leaf
                        let leaf_copy = *root_node;
                        *root_node = *new_leaf;
                        return Ok((root, Some(leaf_copy)));
                    }
                    let shared_prefix_len: u32 = (root_node.key ^ new_leaf.key).leading_zeros();

                    shared_prefix_len
                }
            };

            // change the root in place to represent the LCA of [new_leaf] and [root]
            let crit_bit_mask: u128 = (1u128 << 127) >> shared_prefix_len;
            let new_leaf_crit_bit = (crit_bit_mask & new_leaf.key) != 0;
            let old_root_crit_bit = !new_leaf_crit_bit;

            let new_leaf_handle = self.allocate_leaf().map_err(|_| AoError::SlabOutOfSpace)?;
            self.leaf_nodes[new_leaf_handle as usize] = *new_leaf;

            let new_root_node_handle = self.allocate_inner_node().unwrap();
            let new_root_node = &mut self.inner_nodes[(!new_root_node_handle) as usize];
            new_root_node.prefix_len = shared_prefix_len as u64;
            new_root_node.key = new_leaf.key;
            new_root_node.children[new_leaf_crit_bit as usize] = new_leaf_handle;
            new_root_node.children[old_root_crit_bit as usize] = root;

            if let Some(n) = parent_node {
                let node = &mut self.inner_nodes[(!n) as usize];
                node.children[previous_critbit.unwrap() as usize] = new_root_node_handle;
            } else {
                self.header.root_node = new_root_node_handle;
            }
            self.header.leaf_count += 1;
            return Ok((new_leaf_handle, None));
        }
    }

    #[inline(always)]
    pub fn get_callback_info(&self, leaf_handle: NodeHandle) -> &C {
        &self.callback_infos[leaf_handle as usize]
    }

    #[inline(always)]
    pub fn get_callback_info_mut(&mut self, leaf_handle: NodeHandle) -> &mut C {
        &mut self.callback_infos[leaf_handle as usize]
    }

    pub fn remove_by_key(&mut self, search_key: u128) -> Option<(LeafNode, &C)> {
        let mut grandparent_h: Option<NodeHandle> = None;
        if self.header.leaf_count == 0 {
            return None;
        }
        let mut parent_h = self.header.root_node;
        // We have to initialize the values to work around the type checker
        let mut child_h = 0;
        let mut crit_bit = false;
        let mut prev_crit_bit: Option<bool> = None;
        let mut remove_root = None;
        // let mut depth = 0;
        {
            match Node::from_handle(parent_h) {
                Node::Leaf => {
                    let leaf = &self.leaf_nodes[parent_h as usize];
                    if leaf.key == search_key {
                        remove_root = Some(*leaf);
                    }
                }
                Node::Inner => {
                    let node = self.inner_nodes[(!parent_h) as usize];
                    let (ch, cb) = node.walk_down(search_key);
                    child_h = ch;
                    crit_bit = cb;
                }
            }
        }
        if let Some(leaf_copy) = remove_root {
            self.free_leaf(parent_h);

            self.header.root_node = 0;
            self.header.leaf_count = 0;
            return Some((leaf_copy, self.get_callback_info(parent_h)));
        }
        loop {
            match Node::from_handle(child_h) {
                Node::Inner => {
                    let inner = self.inner_nodes[(!child_h) as usize];
                    let (grandchild_h, grandchild_crit_bit) = inner.walk_down(search_key);
                    grandparent_h = Some(parent_h);
                    parent_h = child_h;
                    child_h = grandchild_h;
                    prev_crit_bit = Some(crit_bit);
                    crit_bit = grandchild_crit_bit;
                    // depth += 1;
                    continue;
                }
                Node::Leaf => {
                    let leaf = &self.leaf_nodes[child_h as usize];
                    if leaf.key != search_key {
                        return None;
                    }

                    break;
                }
            }
        }

        // replace parent with its remaining child node
        // free child_h, replace *parent_h with *other_child_h, free other_child_h
        let other_child_h = self.inner_nodes[(!parent_h) as usize].children[!crit_bit as usize];

        match grandparent_h {
            Some(h) => {
                let r = &mut self.inner_nodes[(!h) as usize];
                r.children[prev_crit_bit.unwrap() as usize] = other_child_h;
            }
            None => self.header.root_node = other_child_h,
        }
        self.header.leaf_count -= 1;
        let removed_leaf = self.leaf_nodes[child_h as usize];
        self.free_leaf(child_h);
        self.free_inner_node(parent_h);
        Some((removed_leaf, self.get_callback_info(child_h)))
    }

    fn find_min_max(&self, find_max: bool) -> Option<NodeHandle> {
        if self.header.leaf_count == 0 {
            return None;
        }
        let mut root: NodeHandle = self.header.root_node;
        loop {
            match Node::from_handle(root) {
                Node::Leaf => return Some(root),
                Node::Inner => {
                    let node = self.inner_nodes[(!root) as usize];
                    root = node.children[if find_max { 1 } else { 0 }];
                }
            }
        }
    }

    #[doc(hidden)]
    pub fn find_min(&self) -> Option<NodeHandle> {
        self.find_min_max(false)
    }

    #[doc(hidden)]
    pub fn find_max(&self) -> Option<NodeHandle> {
        self.find_min_max(true)
    }

    /// Get a price ascending or price descending iterator over all the Slab's orders
    pub fn into_iter(self, price_ascending: bool) -> SlabIterator<'a, C> {
        SlabIterator {
            search_stack: if self.header.leaf_count == 0 {
                vec![]
            } else {
                vec![self.header.root_node]
            },
            slab: self,
            ascending: price_ascending,
        }
    }

    #[cfg(feature = "utils")]
    pub fn get_depth(&self) -> usize {
        if self.header.leaf_count == 0 {
            return 0;
        }
        let mut stack = vec![(self.header.root_node, 1)];
        let mut max_depth = 0;
        while let Some((current_node, current_depth)) = stack.pop() {
            match Node::from_handle(current_node) {
                Node::Inner => {
                    let node = self.inner_nodes[(!current_node) as usize];
                    stack.push((node.children[0], current_depth + 1));
                    stack.push((node.children[1], current_depth + 1));
                }
                Node::Leaf => max_depth = std::cmp::max(current_depth, max_depth),
            }
        }
        max_depth
    }

    #[cfg(test)]
    fn dump(&self) {
        // println!("Callback info length {:?}", self.callback_info_len);
        println!("Header (parsed):");
        let mut header_data = Vec::new();
        println!("{:?}", *self.header);
        self.header.serialize(&mut header_data).unwrap();
        for (k, leaf_node) in self.leaf_nodes.iter().enumerate() {
            println!("Leaf key {:?}", k);
            println!("{:?}", leaf_node);
        }

        for (k, inner_node) in self.inner_nodes.iter().enumerate() {
            println!("Inner Node index {:?}, key {:?}", k, !(k as u32));
            println!("{:?}", inner_node);
        }
    }

    #[cfg(test)]
    fn check_invariants(&self) {
        // first check the live tree contents
        let mut leaf_count = 0;
        let mut inner_node_count = 0;
        fn check_rec<'a, C>(
            slab: &Slab<'a, C>,
            h: NodeHandle,
            last_prefix_len: u64,
            last_prefix: u128,
            last_critbit: bool,
            leaf_count: &mut u64,
            inner_node_count: &mut u64,
        ) {
            match Node::from_handle(h) {
                Node::Leaf => {
                    *leaf_count += 1;
                    let node = &slab.leaf_nodes[h as usize];
                    assert_eq!(
                        last_critbit,
                        (node.key & ((1u128 << 127) >> last_prefix_len)) != 0
                    );
                    let prefix_mask =
                        (((((1u128) << 127) as i128) >> last_prefix_len) as u128) << 1;
                    assert_eq!(last_prefix & prefix_mask, node.key & prefix_mask);
                }
                Node::Inner => {
                    *inner_node_count += 1;
                    let node = &slab.inner_nodes[(!h) as usize];

                    assert!(node.prefix_len > last_prefix_len);
                    assert_eq!(
                        last_critbit,
                        (node.key & ((1u128 << 127) >> last_prefix_len)) != 0
                    );
                    let prefix_mask =
                        (((((1u128) << 127) as i128) >> last_prefix_len) as u128) << 1;
                    assert_eq!(last_prefix & prefix_mask, node.key & prefix_mask);
                    check_rec(
                        slab,
                        node.children[0],
                        node.prefix_len,
                        node.key,
                        false,
                        leaf_count,
                        inner_node_count,
                    );
                    check_rec(
                        slab,
                        node.children[1],
                        node.prefix_len,
                        node.key,
                        true,
                        leaf_count,
                        inner_node_count,
                    );
                }
            }
        }
        if let Some(root) = self.root() {
            if matches!(Node::from_handle(root), Node::Inner) {
                inner_node_count += 1;
                let n = &self.inner_nodes[(!root) as usize];
                check_rec(
                    self,
                    n.children[0],
                    n.prefix_len,
                    n.key,
                    false,
                    &mut leaf_count,
                    &mut inner_node_count,
                );
                check_rec(
                    self,
                    n.children[1],
                    n.prefix_len,
                    n.key,
                    true,
                    &mut leaf_count,
                    &mut inner_node_count,
                );
            } else {
                leaf_count += 1;
            }
        }
        assert_eq!(
            inner_node_count + self.header.inner_node_free_list_len as u64,
            self.header.inner_node_bump_index as u64
        );
        assert_eq!(
            self.header.leaf_count as u64 + self.header.leaf_free_list_len as u64,
            self.header.leaf_bump_index as u64
        );
        assert_eq!(leaf_count, self.header.leaf_count as u64);
    }

    /////////////////////////////////////////
    // Misc

    #[cfg(any(test, feature = "utils"))]
    pub fn find_by_key(&self, search_key: u128) -> Option<NodeHandle> {
        let mut node_handle: NodeHandle = self.root()?;
        loop {
            match Node::from_handle(node_handle) {
                Node::Leaf => {
                    let n = self.leaf_nodes[node_handle as usize];
                    if search_key == n.key {
                        return Some(node_handle);
                    } else {
                        return None;
                    }
                }
                Node::Inner => {
                    let n = self.inner_nodes[(!node_handle as usize)];
                    let common_prefix_len = (search_key ^ n.key).leading_zeros();
                    if common_prefix_len < n.prefix_len as u32 {
                        return None;
                    }
                    node_handle = n.walk_down(search_key).0;
                }
            }
        }
    }
}

impl<'queue, C: Clone> Slab<'queue, C> {
    #[cfg(test)]
    fn traverse(&self) -> Vec<(LeafNode, C)> {
        fn walk_rec<'a, C: Clone>(
            slab: &Slab<'a, C>,
            sub_root: NodeHandle,
            buf: &mut Vec<(LeafNode, C)>,
        ) {
            match Node::from_handle(sub_root) {
                Node::Leaf => {
                    let callback_info = slab.get_callback_info(sub_root);
                    buf.push((slab.leaf_nodes[sub_root as usize], callback_info.clone()));
                }
                Node::Inner => {
                    let n = slab.inner_nodes[(!sub_root) as usize];
                    walk_rec::<C>(slab, n.children[0], buf);
                    walk_rec::<C>(slab, n.children[1], buf);
                }
            }
        }

        let mut buf = Vec::with_capacity(self.header.leaf_count as usize);
        if let Some(r) = self.root() {
            walk_rec(self, r, &mut buf);
        }
        if buf.len() != buf.capacity() {
            self.dump();
        }
        assert_eq!(buf.len(), buf.capacity());
        buf
    }
}

pub struct SlabIterator<'a, C> {
    slab: Slab<'a, C>,
    search_stack: Vec<u32>,
    ascending: bool,
}

impl<'a, C: Pod> Iterator for SlabIterator<'a, C> {
    type Item = LeafNode;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(current) = self.search_stack.pop() {
            match Node::from_handle(current) {
                Node::Inner => {
                    let n = &self.slab.inner_nodes[(!current) as usize];
                    self.search_stack.push(n.children[self.ascending as usize]);
                    self.search_stack.push(n.children[!self.ascending as usize]);
                }
                Node::Leaf => return Some(self.slab.leaf_nodes[current as usize]),
            }
        }
        None
    }
}

/////////////////////////////////////
// Tests

#[cfg(test)]
mod tests {

    use crate::state::orderbook::CallbackInfo;

    use super::*;
    use rand::prelude::*;

    // #[test]
    // fn test_node_serialization() {
    //     let mut rng = StdRng::seed_from_u64(42);
    //     let mut bytes = [0u8; 100];
    //     let mut w: &mut [u8] = &mut bytes;
    //     let l = LeafNode::new(rng.gen(), rng.gen::<[u8; 32]>().to_vec(), rng.gen());
    //     l.serialize(&mut w).unwrap();
    //     let new_leaf = LeafNode::deserialize(&bytes, 32).unwrap();
    //     assert_eq!(l, new_leaf);
    //     let node = NodeTag::Leaf(l);
    //     w = &mut bytes;
    //     node.serialize(&mut &mut w).unwrap();
    //     let new_node = NodeTag::deserialize(&bytes, 32).unwrap();
    //     assert_eq!(node, new_node);
    // }

    #[test]
    fn simulate_find_min() {
        use std::collections::BTreeMap;

        #[derive(Copy, Clone, Zeroable, Pod, Debug, PartialEq)]
        #[repr(C)]
        struct TestCallbackInfo {
            key: [u8; 32],
        }

        impl CallbackInfo for TestCallbackInfo {
            type CallbackId = Self;

            fn as_callback_id(&self) -> &Self::CallbackId {
                self
            }
        }

        for trial in 0..10u64 {
            let mut bytes = vec![0u8; Slab::<[u8; 32]>::compute_allocation_size(10_000)];
            bytes[0] = AccountTag::Asks as u8;
            let mut slab = Slab::from_buffer(&mut bytes, AccountTag::Asks).unwrap();

            slab.header.market_address = Pubkey::new_unique().to_bytes();

            let mut model: BTreeMap<u128, (LeafNode, TestCallbackInfo)> = BTreeMap::new();

            let mut all_keys = vec![];

            let mut rng = StdRng::seed_from_u64(trial);

            assert_eq!(slab.find_min(), None);
            assert_eq!(slab.find_max(), None);

            for i in 0..100 {
                let key = rng.gen();
                let owner = Pubkey::new_unique();
                let qty = rng.gen();
                let leaf = LeafNode {
                    key,
                    base_quantity: qty,
                };

                println!("key : {:x}", key);
                println!("owner : {:?}", &owner.to_bytes());
                println!("{}", i);
                let h = slab.insert_leaf(&leaf).unwrap().0;
                let callback_info = TestCallbackInfo {
                    key: owner.to_bytes(),
                };
                *slab.get_callback_info_mut(h) = callback_info;
                model
                    .insert(key, (leaf, callback_info))
                    .ok_or(())
                    .unwrap_err();
                all_keys.push(key);

                // test find_by_key
                let valid_search_key = *all_keys.choose(&mut rng).unwrap();
                let invalid_search_key = rng.gen();

                for &search_key in &[valid_search_key, invalid_search_key] {
                    let slab_value = slab.find_by_key(search_key).map(|x| {
                        let s = slab.leaf_nodes[x as usize];
                        (s.to_owned(), *slab.get_callback_info(x))
                    });
                    let model_value = model.get(&search_key).cloned();
                    assert_eq!(slab_value, model_value);
                }

                // test find_min
                let min_h = slab.find_min().unwrap();
                let slab_min = slab.leaf_nodes[min_h as usize];
                let model_min = model.iter().next().unwrap().1;
                let owner = *slab.get_callback_info(min_h);
                assert_eq!(&(slab_min, owner), model_min);

                // test find_max
                let max_h = slab.find_max().unwrap();
                let slab_max = slab.leaf_nodes[max_h as usize];
                let model_max = model.iter().next_back().unwrap().1;
                let owner = *slab.get_callback_info(max_h);
                assert_eq!(&(slab_max, owner), model_max);
            }
        }
    }

    #[test]
    #[cfg(not(feature = "quick-test"))]
    fn simulate_operations() {
        use rand::distributions::WeightedIndex;
        use std::collections::BTreeMap;

        let mut bytes = vec![0u8; Slab::<[u8; 32]>::compute_allocation_size(10_000)];
        bytes[0] = AccountTag::Asks as u8;
        let mut slab = Slab::from_buffer(&mut bytes, AccountTag::Asks).unwrap();

        slab.header.market_address = Pubkey::new_unique().to_bytes();
        let mut model: BTreeMap<u128, (LeafNode, [u8; 32])> = BTreeMap::new();

        let mut all_keys = vec![];
        let mut rng = StdRng::seed_from_u64(1);

        #[derive(Copy, Clone, Debug)]
        enum Op {
            InsertNew,
            InsertDup,
            Delete,
            Min,
            Max,
            End,
        }

        for weights in &[
            [
                (Op::InsertNew, 2000),
                (Op::InsertDup, 200),
                (Op::Delete, 2210),
                (Op::Min, 500),
                (Op::Max, 500),
                (Op::End, 1),
            ],
            [
                (Op::InsertNew, 10),
                (Op::InsertDup, 200),
                (Op::Delete, 5210),
                (Op::Min, 500),
                (Op::Max, 500),
                (Op::End, 1),
            ],
        ] {
            let dist = WeightedIndex::new(weights.iter().map(|(_op, wt)| wt)).unwrap();

            for i in 0..100_000 {
                slab.check_invariants();
                let model_state = model.values().collect::<Vec<_>>();
                let slab_state: Vec<(LeafNode, [u8; 32])> = slab.traverse();
                assert_eq!(model_state, slab_state.iter().collect::<Vec<_>>());
                let op = weights[dist.sample(&mut rng)].0;
                println!("Operation : {:?}", op);

                match op {
                    Op::InsertNew | Op::InsertDup => {
                        let key = match op {
                            Op::InsertNew => rng.gen(),
                            Op::InsertDup => *all_keys.choose(&mut rng).unwrap(),
                            _ => unreachable!(),
                        };
                        let owner = Pubkey::new_unique().to_bytes();
                        let qty = rng.gen();
                        let leaf = LeafNode {
                            key,
                            base_quantity: qty,
                        };
                        let (leaf_h, old_leaf) = slab.insert_leaf(&leaf).unwrap();
                        let old_owner = *slab.get_callback_info(leaf_h);
                        *slab.get_callback_info_mut(leaf_h) = owner;

                        println!("Insert {:x}", key);

                        all_keys.push(key);
                        let slab_value = old_leaf.map(|l| (l, old_owner));
                        let model_value = model.insert(key, (leaf, owner));
                        if slab_value != model_value {
                            slab.dump();
                        }
                        assert_eq!(slab_value, model_value);
                    }
                    Op::Delete => {
                        let key = all_keys
                            .choose(&mut rng)
                            .copied()
                            .unwrap_or_else(|| rng.gen());

                        println!("Remove {:x}", key);

                        let slab_value = slab.remove_by_key(key).map(|v| v.0);
                        let model_value = model.remove(&key).map(|(n, _)| n);
                        assert_eq!(slab_value, model_value);
                    }
                    Op::Min => {
                        if model.is_empty() {
                            assert_eq!(slab.header.leaf_count, 0);
                        } else {
                            let slab_min_h = slab.find_min().unwrap();
                            let slab_min = slab.leaf_nodes[slab_min_h as usize];
                            let owner = *slab.get_callback_info(slab_min_h);
                            let model_min = model.iter().next().unwrap().1;
                            assert_eq!(&(slab_min, owner), model_min);
                        }
                    }
                    Op::Max => {
                        if model.is_empty() {
                            assert_eq!(slab.header.leaf_count, 0);
                        } else {
                            let slab_max_h = slab.find_max().unwrap();
                            let slab_max = slab.leaf_nodes[slab_max_h as usize];
                            let owner = *slab.get_callback_info(slab_max_h);
                            let model_max = model.iter().next_back().unwrap().1;
                            assert_eq!(&(slab_max, owner), model_max);
                        }
                    }
                    Op::End => {
                        if i > 10_000 {
                            break;
                        }
                    }
                }
            }
        }
    }
}
