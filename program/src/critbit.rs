use crate::error::AOError;
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::cast;
use fixed::types::extra::U32;
use fixed::FixedU64;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use std::convert::{TryFrom, TryInto};
use std::io::Write;
use std::{cell::RefCell, convert::identity, mem::size_of, rc::Rc};
// A Slab contains the data for a slab header and an array of nodes of a critbit tree
// whose leafs contain the data referencing an order of the orderbook.

////////////////////////////////////
// Nodes
//TODO make node tags u8

pub type NodeHandle = u32;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
enum NodeTag {
    Uninitialized,
    InnerNode,
    LeafNode,
    FreeNode,
    LastFreeNode,
}

#[derive(BorshDeserialize, BorshSerialize)]
struct InnerNode {
    prefix_len: u32,
    key: u128,
    children: [u32; 2],
}

impl InnerNode {
    fn walk_down(&self, search_key: u128) -> (NodeHandle, bool) {
        let crit_bit_mask = (1u128 << 127) >> self.prefix_len;
        let crit_bit = (search_key & crit_bit_mask) != 0;
        (self.children[crit_bit as usize], crit_bit)
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
pub struct LeafNode {
    pub key: u128,
    pub callback_info: Vec<u8>,
    pub asset_quantity: u64,
}

impl LeafNode {
    pub fn serialize<W: Write>(&self, writer: &mut W) {
        writer.write(&self.key.to_le_bytes());
        writer.write(&self.callback_info);
        writer.write(&self.asset_quantity.to_le_bytes());
    }

    pub fn deserialize(buf: &mut &[u8], callback_info_len: usize) -> Self {
        let key = u128::from_le_bytes(buf[..16].try_into().unwrap());
        let callback_info = buf[16..callback_info_len + 16].to_owned();
        let quantity = u64::from_le_bytes(
            buf[callback_info_len + 16..callback_info_len + 24]
                .try_into()
                .unwrap(),
        );
        *buf = &buf[callback_info_len + 24..];
        Self {
            key,
            callback_info,
            asset_quantity: quantity,
        }
    }
}

pub const NODE_DATA_SIZE: usize = size_of::<LeafNode>(); //TODO Change to hardcoded
                                                         // pub const SLOT_SIZE: usize = size_of::<LeafNode>() + 4; // Account for the tag
pub const INNER_NODE_SIZE: usize = 28;

impl LeafNode {
    pub fn new(key: u128, callback_info: Vec<u8>, quantity: u64) -> Self {
        LeafNode {
            key,
            callback_info,
            asset_quantity: quantity,
        }
    }

    pub fn price(&self) -> u64 {
        (self.key >> 64) as u64
    }

    pub fn order_id(&self) -> u128 {
        self.key
    }

    pub fn set_asset_quantity(&mut self, quantity: u64) {
        self.asset_quantity = quantity;
    }

    pub fn to_any(&self) -> AnyNode {
        //TODO retire
        let mut data = Vec::new();
        self.serialize(&mut data);
        AnyNode {
            tag: NodeTag::LeafNode.into(),
            data,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
struct FreeNode {
    next: u32,
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct AnyNode {
    tag: u32,
    data: Vec<u8>,
}

enum Node {
    Inner(InnerNode),
    Leaf(LeafNode),
}

impl<'a> AnyNode {
    fn key(&self, callback_info_len: usize) -> Option<u128> {
        match self.case(callback_info_len)? {
            Node::Inner(inner) => Some(inner.key),
            Node::Leaf(leaf) => Some(leaf.key),
        }
    }

    #[cfg(test)]
    fn prefix_len(&self, callback_info_len: usize) -> u32 {
        match self.case(callback_info_len).unwrap() {
            Node::Inner(InnerNode { prefix_len, .. }) => prefix_len,
            Node::Leaf(_) => 128,
        }
    }

    fn children(&self, callback_info_len: usize) -> Option<[u32; 2]> {
        match self.case(callback_info_len).unwrap() {
            Node::Inner(InnerNode { children, .. }) => Some(children),
            Node::Leaf(_) => None,
        }
    }

    fn case(&'a self, callback_info_len: usize) -> Option<Node> {
        match NodeTag::try_from(self.tag) {
            Ok(NodeTag::InnerNode) => {
                let inner_node = InnerNode::deserialize(&mut &self.data[..]).unwrap();
                Some(Node::Inner(inner_node))
            }
            Ok(NodeTag::LeafNode) => {
                let leaf_node = LeafNode::deserialize(&mut &self.data[..], callback_info_len);
                Some(Node::Leaf(leaf_node))
            }
            _ => None,
        }
    }

    pub fn as_leaf(&self, callback_info_len: usize) -> Option<LeafNode> {
        match self.case(callback_info_len) {
            Some(Node::Leaf(leaf_ref)) => Some(leaf_ref),
            _ => None,
        }
    }
}

////////////////////////////////////
// Slabs

#[derive(BorshDeserialize, BorshSerialize)]
struct SlabHeader {
    bump_index: u64,
    free_list_len: u64,
    free_list_head: u32,

    root_node: u32,
    leaf_count: u64,
    market_address: Pubkey,
}

const SLAB_HEADER_LEN: usize = size_of::<SlabHeader>();

pub struct Slab<'a> {
    pub buffer: Rc<RefCell<&'a mut [u8]>>,
    pub callback_info_len: usize,
    pub slot_size: usize,
}

// Data access methods
impl<'a> Slab<'a> {
    // pub fn new(bytes: &'a mut [u8]) -> Self {
    //     let len_without_header = bytes.len().checked_sub(SLAB_HEADER_LEN).unwrap();
    //     let slop = len_without_header % size_of::<AnyNode>();
    //     let truncated_len = bytes.len() - slop;
    //     let bytes = &mut bytes[..truncated_len];
    //     Slab(Rc::new(RefCell::new(bytes)))
    // }

    pub fn new_from_acc_info(acc_info: &AccountInfo<'a>, callback_info_len: usize) -> Self {
        let len_without_header = acc_info.data_len().checked_sub(SLAB_HEADER_LEN).unwrap();
        assert_eq!(len_without_header % size_of::<AnyNode>(), 0); // TODO either truncate or throw
        Self {
            buffer: Rc::clone(&acc_info.data),
            callback_info_len,
            slot_size: Self::compute_slot_size(callback_info_len),
        }
    }

    fn get_header(&self) -> SlabHeader {
        SlabHeader::deserialize(&mut &self.buffer.borrow()[..SLAB_HEADER_LEN]).unwrap()
    }

    fn compute_slot_size(callback_info_len: usize) -> usize {
        std::cmp::max(callback_info_len + 8 + 16, INNER_NODE_SIZE)
    }
}

// Tree nodes manipulation methods
impl<'a> Slab<'a> {
    fn capacity(&self) -> u64 {
        ((self.buffer.borrow().len() - SLAB_HEADER_LEN) % self.slot_size) as u64
    }

    pub(crate) fn get_node(&self, key: u32) -> Option<AnyNode> {
        let offset = SLAB_HEADER_LEN + key as usize;
        let node =
            AnyNode::deserialize(&mut &self.buffer.borrow()[offset..offset + self.slot_size])
                .unwrap();
        let tag = NodeTag::try_from(node.tag);
        match tag {
            Ok(NodeTag::InnerNode) | Ok(NodeTag::LeafNode) => Some(node),
            _ => None,
        }
    }

    fn write_node(&mut self, node: &AnyNode, key: u32) {
        let offset = SLAB_HEADER_LEN + key as usize;
        self.buffer.borrow_mut()[offset..offset + 4].copy_from_slice(&node.tag.to_le_bytes()); //TODO
        self.buffer.borrow_mut()[offset + 4..offset + self.slot_size].copy_from_slice(&node.data);
    }

    fn insert(&mut self, val: &AnyNode) -> Result<u32, ()> {
        match NodeTag::try_from(identity(val.tag)) {
            Ok(NodeTag::InnerNode) | Ok(NodeTag::LeafNode) => (),
            _ => unreachable!(),
        };

        let mut header = self.get_header();

        if header.free_list_len == 0 {
            if header.bump_index as usize == self.capacity() as usize {
                return Err(());
            }

            if header.bump_index == std::u32::MAX as u64 {
                return Err(());
            }
            let key = header.bump_index as u32;
            header.bump_index += 1;

            self.write_node(val, key);
            return Ok(key);
        }

        let key = header.free_list_head;
        let node = self.get_node(key).unwrap();

        match NodeTag::try_from(node.tag) {
            Ok(NodeTag::FreeNode) => assert!(header.free_list_len > 1),
            Ok(NodeTag::LastFreeNode) => assert_eq!(identity(header.free_list_len), 1),
            _ => unreachable!(),
        };

        let next_free_list_head: u32;
        {
            let free_list_item = FreeNode::deserialize(&mut &node.data[..]).unwrap();
            next_free_list_head = free_list_item.next;
        }
        header.free_list_head = next_free_list_head;
        header.free_list_len -= 1;

        self.write_node(val, key);
        Ok(key)
    }

    fn remove(&mut self, key: u32) -> Option<AnyNode> {
        let val = self.get_node(key)?;
        let mut header = self.get_header();
        let mut any_node_ref = self.get_node(key).unwrap();
        any_node_ref.tag = if header.free_list_len == 0 {
            NodeTag::LastFreeNode.into()
        } else {
            NodeTag::FreeNode.into()
        };
        let free_node_ref = FreeNode {
            next: header.free_list_head,
        };
        header.free_list_len += 1;
        header.free_list_head = key;
        let mut free_node_data = Vec::with_capacity(NODE_DATA_SIZE);
        free_node_ref.serialize(&mut free_node_data).unwrap();
        self.write_node(
            &AnyNode {
                tag: any_node_ref.tag,
                data: free_node_data,
            },
            key,
        );

        Some(val)
    }
}

// Critbit tree walks
impl<'a> Slab<'a> {
    fn root(&self) -> Option<NodeHandle> {
        if self.get_header().leaf_count == 0 {
            return None;
        }

        Some(self.get_header().root_node)
    }

    fn find_min_max(&self, find_max: bool) -> Option<NodeHandle> {
        let mut root: NodeHandle = self.root()?;
        loop {
            let root_contents = self.get_node(root).unwrap();
            match root_contents.case(self.callback_info_len).unwrap() {
                Node::Inner(InnerNode { children, .. }) => {
                    root = children[if find_max { 1 } else { 0 }];
                    continue;
                }
                _ => return Some(root),
            }
        }
    }

    pub fn find_min(&self) -> Option<NodeHandle> {
        self.find_min_max(false)
    }

    pub fn find_max(&self) -> Option<NodeHandle> {
        self.find_min_max(true)
    }

    pub fn insert_leaf(
        &mut self,
        new_leaf: &LeafNode,
    ) -> Result<(NodeHandle, Option<LeafNode>), AOError> {
        let mut root: NodeHandle = match self.root() {
            Some(h) => h,
            None => {
                // create a new root if none exists
                match self.insert(&new_leaf.to_any()) {
                    Ok(handle) => {
                        self.get_header().root_node = handle;
                        self.get_header().leaf_count = 1;
                        return Ok((handle, None));
                    }
                    Err(()) => return Err(AOError::SlabOutOfSpace),
                }
            }
        };
        loop {
            // check if the new node will be a child of the root
            let root_contents = self.get_node(root).unwrap();
            let root_key = root_contents.key(self.callback_info_len).unwrap();
            if root_key == new_leaf.key {
                if let Some(Node::Leaf(old_root_as_leaf)) =
                    root_contents.case(self.callback_info_len)
                {
                    // clobber the existing leaf
                    self.write_node(&new_leaf.to_any(), root);
                    return Ok((root, Some(old_root_as_leaf)));
                }
            }
            let shared_prefix_len: u32 = (root_key ^ new_leaf.key).leading_zeros();
            match root_contents.case(self.callback_info_len) {
                None => unreachable!(),
                Some(Node::Inner(inner)) => {
                    let keep_old_root = shared_prefix_len >= inner.prefix_len;
                    if keep_old_root {
                        root = inner.walk_down(new_leaf.key).0;
                        continue;
                    };
                }
                _ => (),
            };

            // change the root in place to represent the LCA of [new_leaf] and [root]
            let crit_bit_mask: u128 = (1u128 << 127) >> shared_prefix_len;
            let new_leaf_crit_bit = (crit_bit_mask & new_leaf.key) != 0;
            let old_root_crit_bit = !new_leaf_crit_bit;

            let new_leaf_handle = self
                .insert(&new_leaf.to_any())
                .map_err(|()| AOError::SlabOutOfSpace)?;
            let moved_root_handle = match self.insert(&root_contents) {
                Ok(h) => h,
                Err(()) => {
                    self.remove(new_leaf_handle).unwrap();
                    return Err(AOError::SlabOutOfSpace);
                }
            };

            let mut root_node = InnerNode {
                prefix_len: shared_prefix_len,
                key: new_leaf.key,
                children: [0; 2],
            };

            root_node.children[new_leaf_crit_bit as usize] = new_leaf_handle;
            root_node.children[old_root_crit_bit as usize] = moved_root_handle;

            let mut root_node_data = Vec::new();
            root_node.serialize(&mut root_node_data).unwrap();
            let root_node_any = AnyNode {
                tag: NodeTag::InnerNode.into(),
                data: root_node_data,
            };
            self.write_node(&root_node_any, root);

            self.get_header().leaf_count += 1;
            return Ok((new_leaf_handle, None));
        }
    }

    pub fn remove_by_key(&mut self, search_key: u128) -> Option<LeafNode> {
        let mut parent_h = self.root()?;
        let mut child_h;
        let mut crit_bit;
        match self
            .get_node(parent_h)
            .unwrap()
            .case(self.callback_info_len)
            .unwrap()
        {
            Node::Leaf(leaf) if leaf.key == search_key => {
                let mut header = self.get_header();
                assert_eq!(identity(header.leaf_count), 1);
                header.root_node = 0;
                header.leaf_count = 0;
                let _old_root = self.remove(parent_h).unwrap();
                return Some(leaf);
            }
            Node::Leaf(_) => return None,
            Node::Inner(inner) => {
                let (ch, cb) = inner.walk_down(search_key);
                child_h = ch;
                crit_bit = cb;
            }
        }
        loop {
            match self
                .get_node(child_h)
                .unwrap()
                .case(self.callback_info_len)
                .unwrap()
            {
                Node::Inner(inner) => {
                    let (grandchild_h, grandchild_crit_bit) = inner.walk_down(search_key);
                    parent_h = child_h;
                    child_h = grandchild_h;
                    crit_bit = grandchild_crit_bit;
                    continue;
                }
                Node::Leaf(leaf) => {
                    if leaf.key != search_key {
                        return None;
                    }

                    break;
                }
            }
        }
        // replace parent with its remaining child node
        // free child_h, replace *parent_h with *other_child_h, free other_child_h
        let other_child_h = self
            .get_node(parent_h)
            .unwrap()
            .children(self.callback_info_len)
            .unwrap()[!crit_bit as usize];
        let other_child_node_contents = self.remove(other_child_h).unwrap();
        self.write_node(&other_child_node_contents, parent_h);
        self.get_header().leaf_count -= 1;
        let removed_leaf = LeafNode::deserialize(
            &mut &self.remove(child_h).unwrap().data[..],
            self.callback_info_len,
        );
        Some(removed_leaf)
    }

    pub fn remove_min(&mut self) -> Option<LeafNode> {
        self.remove_by_key(
            self.get_node(self.find_min()?)?
                .key(self.callback_info_len)?,
        )
    }

    pub fn remove_max(&mut self) -> Option<LeafNode> {
        self.remove_by_key(
            self.get_node(self.find_max()?)?
                .key(self.callback_info_len)?,
        )
    }

    /////////////////////////////////////////
    // Misc

    #[cfg(test)]
    fn find_by_key(&self, search_key: u128) -> Option<NodeHandle> {
        let mut node_handle: NodeHandle = self.root()?;
        loop {
            let node_ref = self.get_node(node_handle).unwrap();
            let node_prefix_len = node_ref.prefix_len(self.callback_info_len);
            let node_key = node_ref.key(self.callback_info_len).unwrap();
            let common_prefix_len = (search_key ^ node_key).leading_zeros();
            if common_prefix_len < node_prefix_len {
                return None;
            }
            match node_ref.case(self.callback_info_len).unwrap() {
                Node::Leaf(_) => break Some(node_handle),
                Node::Inner(inner) => {
                    let crit_bit_mask = (1u128 << 127) >> node_prefix_len;
                    let _search_key_crit_bit = (search_key & crit_bit_mask) != 0;
                    node_handle = inner.walk_down(search_key).0;
                    continue;
                }
            }
        }
    }

    #[cfg(test)]
    fn traverse(&self) -> Vec<LeafNode> {
        fn walk_rec<'a>(slab: &'a Slab, sub_root: NodeHandle, buf: &mut Vec<LeafNode>) {
            match slab
                .get_node(sub_root)
                .unwrap()
                .case(slab.callback_info_len)
                .unwrap()
            {
                Node::Leaf(leaf) => {
                    buf.push(leaf);
                }
                Node::Inner(inner) => {
                    walk_rec(slab, inner.children[0], buf);
                    walk_rec(slab, inner.children[1], buf);
                }
            }
        }

        let mut buf = Vec::with_capacity(self.get_header().leaf_count as usize);
        if let Some(r) = self.root() {
            walk_rec(self, r, &mut buf);
        }
        if buf.len() != buf.capacity() {
            self.hexdump();
        }
        assert_eq!(buf.len(), buf.capacity());
        buf
    }

    #[cfg(test)]
    fn hexdump(&self) {
        println!("Header:");
        let mut header_data = Vec::new();
        self.get_header().serialize(&mut header_data).unwrap();
        hexdump::hexdump(&header_data);
        println!("Data:");
        hexdump::hexdump(&self.buffer.borrow());
    }

    #[cfg(test)]
    fn check_invariants(&self) {
        // first check the live tree contents
        let mut count = 0;
        fn check_rec(
            slab: &Slab,
            key: NodeHandle,
            last_prefix_len: u32,
            last_prefix: u128,
            last_crit_bit: bool,
            count: &mut u64,
        ) {
            *count += 1;
            let node = slab.get_node(key).unwrap();
            assert!(node.prefix_len(slab.callback_info_len) > last_prefix_len);
            let node_key = node.key(slab.callback_info_len).unwrap();
            assert_eq!(
                last_crit_bit,
                (node_key & ((1u128 << 127) >> last_prefix_len)) != 0
            );
            let prefix_mask = (((((1u128) << 127) as i128) >> last_prefix_len) as u128) << 1;
            assert_eq!(
                last_prefix & prefix_mask,
                node.key(slab.callback_info_len).unwrap() & prefix_mask
            );
            if let Some([c0, c1]) = node.children(slab.callback_info_len) {
                check_rec(
                    slab,
                    c0,
                    node.prefix_len(slab.callback_info_len),
                    node_key,
                    false,
                    count,
                );
                check_rec(
                    slab,
                    c1,
                    node.prefix_len(slab.callback_info_len),
                    node_key,
                    true,
                    count,
                );
            }
        }
        if let Some(root) = self.root() {
            count += 1;
            let node = self.get_node(root).unwrap();
            let node_key = node.key(self.callback_info_len).unwrap();
            if let Some([c0, c1]) = node.children(self.callback_info_len) {
                check_rec(
                    self,
                    c0,
                    node.prefix_len(self.callback_info_len),
                    node_key,
                    false,
                    &mut count,
                );
                check_rec(
                    self,
                    c1,
                    node.prefix_len(self.callback_info_len),
                    node_key,
                    true,
                    &mut count,
                );
            }
        }
        assert_eq!(
            count + self.get_header().free_list_len as u64,
            identity(self.get_header().bump_index)
        );

        let mut free_nodes_remaining = self.get_header().free_list_len;
        let mut next_free_node = self.get_header().free_list_head;
        loop {
            let contents;
            match free_nodes_remaining {
                0 => break,
                1 => {
                    contents = self.get_node(next_free_node).unwrap();
                    assert_eq!(identity(contents.tag), u32::from(NodeTag::LastFreeNode));
                }
                _ => {
                    contents = self.get_node(next_free_node).unwrap();
                    assert_eq!(identity(contents.tag), u32::from(NodeTag::FreeNode));
                }
            };
            let typed_ref = FreeNode::deserialize(&mut &contents.data[..]).unwrap();
            next_free_node = typed_ref.next;
            free_nodes_remaining -= 1;
        }
    }
}

/////////////////////////////////////
// Tests

#[cfg(test)]
mod tests {

    use super::*;
    use rand::prelude::*;

    #[test]
    fn simulate_find_min() {
        use std::collections::BTreeMap;

        for trial in 0..10u64 {
            let mut bytes = vec![0u8; 80_000];
            let slab_data = Rc::new(RefCell::new(&mut bytes[..]));
            let mut slab = Slab {
                buffer: slab_data,
                callback_info_len: 32,
                slot_size: Slab::compute_slot_size(32),
            };

            let mut model: BTreeMap<u128, LeafNode> = BTreeMap::new();

            let mut all_keys = vec![];

            let mut rng = StdRng::seed_from_u64(trial);

            assert_eq!(slab.find_min(), None);
            assert_eq!(slab.find_max(), None);

            for i in 0..100 {
                let key = rng.gen();
                let owner = Pubkey::new_unique();
                let qty = rng.gen();
                let leaf = LeafNode::new(key, owner.to_bytes().to_vec(), qty);

                println!("{:x}", key);
                println!("{}", i);

                slab.insert_leaf(&leaf).unwrap();
                model.insert(key, leaf).ok_or(()).unwrap_err();
                all_keys.push(key);

                // test find_by_key
                let valid_search_key = *all_keys.choose(&mut rng).unwrap();
                let invalid_search_key = rng.gen();

                for &search_key in &[valid_search_key, invalid_search_key] {
                    let slab_value = slab
                        .find_by_key(search_key)
                        .map(|x| slab.get_node(x))
                        .flatten()
                        .map(|a| a.as_leaf(slab.callback_info_len))
                        .flatten();
                    let model_value = model.get(&search_key).cloned();
                    assert_eq!(slab_value, model_value);
                }

                // test find_min
                let slab_min = slab
                    .get_node(slab.find_min().unwrap())
                    .map(|a| a.as_leaf(slab.callback_info_len))
                    .flatten()
                    .unwrap();
                let model_min = model.iter().next().unwrap().1;
                assert_eq!(&slab_min, model_min);

                // test find_max
                let slab_max = slab
                    .get_node(slab.find_max().unwrap())
                    .map(|a| a.as_leaf(slab.callback_info_len))
                    .flatten()
                    .unwrap();
                let model_max = model.iter().next_back().unwrap().1;
                assert_eq!(&slab_max, model_max);
            }
        }
    }

    #[test]
    fn simulate_operations() {
        use rand::distributions::WeightedIndex;
        use std::collections::BTreeMap;

        let mut bytes = vec![0u8; 80_000];
        let slab_data = Rc::new(RefCell::new(&mut bytes[..]));
        let mut slab = Slab {
            buffer: slab_data,
            callback_info_len: 32,
            slot_size: Slab::compute_slot_size(32),
        };
        let mut model: BTreeMap<u128, LeafNode> = BTreeMap::new();

        let mut all_keys = vec![];
        let mut rng = StdRng::seed_from_u64(0);

        #[derive(Copy, Clone)]
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
                let slab_state: Vec<LeafNode> = slab.traverse();
                assert_eq!(model_state, slab_state.iter().collect::<Vec<&LeafNode>>());

                match weights[dist.sample(&mut rng)].0 {
                    op @ Op::InsertNew | op @ Op::InsertDup => {
                        let key = match op {
                            Op::InsertNew => rng.gen(),
                            Op::InsertDup => *all_keys.choose(&mut rng).unwrap(),
                            _ => unreachable!(),
                        };
                        let owner = Pubkey::new_unique();
                        let qty = rng.gen();
                        let leaf = LeafNode::new(key, owner.to_bytes().to_vec(), qty);

                        println!("Insert {:x}", key);

                        all_keys.push(key);
                        let slab_value = slab.insert_leaf(&leaf).unwrap().1;
                        let model_value = model.insert(key, leaf);
                        if slab_value != model_value {
                            slab.hexdump();
                        }
                        assert_eq!(slab_value, model_value);
                    }
                    Op::Delete => {
                        let key = all_keys
                            .choose(&mut rng)
                            .copied()
                            .unwrap_or_else(|| rng.gen());

                        println!("Remove {:x}", key);

                        let slab_value = slab.remove_by_key(key);
                        let model_value = model.remove(&key);
                        assert_eq!(slab_value, model_value);
                    }
                    Op::Min => {
                        if model.is_empty() {
                            assert_eq!(identity(slab.get_header().leaf_count), 0);
                        } else {
                            let slab_min = slab
                                .get_node(slab.find_min().unwrap())
                                .map(|a| a.as_leaf(slab.callback_info_len))
                                .flatten()
                                .unwrap();
                            let model_min = model.iter().next().unwrap().1;
                            assert_eq!(&slab_min, model_min);
                        }
                    }
                    Op::Max => {
                        if model.is_empty() {
                            assert_eq!(identity(slab.get_header().leaf_count), 0);
                        } else {
                            let slab_max = slab
                                .get_node(slab.find_max().unwrap())
                                .map(|a| a.as_leaf(slab.callback_info_len))
                                .flatten()
                                .unwrap();
                            let model_max = model.iter().next_back().unwrap().1;
                            assert_eq!(&slab_max, model_max);
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
