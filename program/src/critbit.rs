use crate::error::AoError;
use crate::state::{AccountTag, Side};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use std::convert::TryInto;
use std::io::Write;
use std::{cell::RefCell, convert::identity, mem::size_of, rc::Rc};
// A Slab contains the data for a slab header and an array of nodes of a critbit tree
// whose leafs contain the data referencing an order of the orderbook.

////////////////////////////////////
// Nodes

pub type NodeHandle = u32;

pub type IoError = std::io::Error;

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone)]
pub struct InnerNode {
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
    pub fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), IoError> {
        writer.write_all(&self.key.to_le_bytes())?;
        writer.write_all(&self.callback_info)?;
        writer.write_all(&self.asset_quantity.to_le_bytes())?;
        Ok(())
    }

    pub fn deserialize(buf: &[u8], callback_info_len: usize) -> Result<Self, IoError> {
        let key = u128::from_le_bytes(
            buf[..16]
                .try_into()
                .map_err(|_| std::io::ErrorKind::InvalidData)?,
        );
        let callback_info = buf[16..callback_info_len + 16].to_owned();
        let asset_quantity = u64::from_le_bytes(
            buf[callback_info_len + 16..callback_info_len + 24]
                .try_into()
                .map_err(|_| std::io::ErrorKind::InvalidData)?,
        );
        Ok(Self {
            key,
            callback_info,
            asset_quantity,
        })
    }
}

pub const INNER_NODE_SIZE: usize = 32;

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
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone)]
pub struct FreeNode {
    next: u32,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Node {
    Uninitialized,
    Inner(InnerNode),
    Leaf(LeafNode),
    Free(FreeNode),
    LastFree(FreeNode),
}

impl<'a> Node {
    pub fn deserialize(buffer: &[u8], callback_info_len: usize) -> Result<Self, IoError> {
        match buffer[0] {
            0 => Ok(Node::Uninitialized),
            1 => Ok(Node::Inner(InnerNode::deserialize(&mut &buffer[1..])?)),
            2 => Ok(Node::Leaf(LeafNode::deserialize(
                &buffer[1..],
                callback_info_len,
            )?)),
            3 => Ok(Node::Free(FreeNode::deserialize(&mut &buffer[1..])?)),
            4 => Ok(Node::LastFree(FreeNode::deserialize(&mut &buffer[1..])?)),
            _ => Err(std::io::ErrorKind::InvalidData.into()),
        }
    }

    pub fn serialize<W: Write>(&self, w: &mut W) -> Result<(), IoError> {
        match self {
            Node::Uninitialized => w.write_all(&[0]),
            Node::Inner(n) => {
                w.write_all(&[1])?;
                n.serialize(w)
            }
            Node::Leaf(n) => {
                w.write_all(&[2])?;
                n.serialize(w)
            }
            Node::Free(n) => {
                w.write_all(&[3])?;
                n.serialize(w)
            }
            Node::LastFree(n) => {
                w.write_all(&[4])?;
                n.serialize(w)
            }
        }
    }
    fn key(&self) -> Option<u128> {
        match &self {
            Node::Inner(inner) => Some(inner.key),
            Node::Leaf(leaf) => Some(leaf.key),
            _ => None,
        }
    }

    #[cfg(test)]
    fn prefix_len(&self) -> Result<u32, IoError> {
        match &self {
            Node::Inner(InnerNode { prefix_len, .. }) => Ok(*prefix_len),
            Node::Leaf(_) => Ok(128),
            _ => Err(std::io::ErrorKind::InvalidData.into()),
        }
    }

    fn children(&self) -> Option<&[u32; 2]> {
        match &self {
            Node::Inner(InnerNode { children, .. }) => Some(&children),
            _ => None,
        }
    }

    pub fn as_leaf(&self) -> Option<&LeafNode> {
        match &self {
            Node::Leaf(leaf_ref) => Some(leaf_ref),
            _ => None,
        }
    }
}

////////////////////////////////////
// Slabs

#[derive(BorshDeserialize, BorshSerialize, Debug)]
struct SlabHeader {
    account_tag: AccountTag,
    bump_index: u64,
    free_list_len: u64,
    free_list_head: u32,

    root_node: u32,
    leaf_count: u64,
    market_address: Pubkey,
}
pub const SLAB_HEADER_LEN: usize = 65;

pub struct Slab<'a> {
    header: SlabHeader,
    pub buffer: Rc<RefCell<&'a mut [u8]>>,
    pub callback_info_len: usize,
    pub slot_size: usize,
}

// Data access methods
impl<'a> Slab<'a> {
    pub fn check(&self, side: Side) -> bool {
        match side {
            Side::Bid => self.header.account_tag == AccountTag::Bids,
            Side::Ask => self.header.account_tag == AccountTag::Asks,
        }
    }
    pub fn new_from_acc_info(acc_info: &AccountInfo<'a>, callback_info_len: usize) -> Self {
        let slot_size = Self::compute_slot_size(callback_info_len);
        // assert_eq!(len_without_header % slot_size, 0);
        Self {
            buffer: Rc::clone(&acc_info.data),
            callback_info_len,
            slot_size,
            header: SlabHeader::deserialize(&mut (&acc_info.data.borrow() as &[u8])).unwrap(),
        }
    }

    pub(crate) fn write_header(&self) {
        self.header
            .serialize(&mut &mut self.buffer.borrow_mut()[..SLAB_HEADER_LEN])
            .unwrap()
    }

    pub fn compute_slot_size(callback_info_len: usize) -> usize {
        std::cmp::max(callback_info_len + 8 + 16 + 1, INNER_NODE_SIZE)
    }

    pub(crate) fn initialize(
        bids_account: &AccountInfo<'a>,
        asks_account: &AccountInfo<'a>,
        market_address: Pubkey,
    ) {
        let mut header = SlabHeader {
            account_tag: AccountTag::Asks,
            bump_index: 0,
            free_list_len: 0,
            free_list_head: 0,
            root_node: 0,
            leaf_count: 0,
            market_address,
        };
        header
            .serialize(&mut ((&mut asks_account.data.borrow_mut()) as &mut [u8]))
            .unwrap();
        header.account_tag = AccountTag::Bids;
        header
            .serialize(&mut ((&mut bids_account.data.borrow_mut()) as &mut [u8]))
            .unwrap();
    }
}

// Tree nodes manipulation methods
impl<'a> Slab<'a> {
    fn capacity(&self) -> u64 {
        ((self.buffer.borrow().len() - SLAB_HEADER_LEN) / self.slot_size) as u64
    }

    pub fn get_node(&self, key: u32) -> Option<Node> {
        let offset = SLAB_HEADER_LEN + (key as usize) * self.slot_size;
        // println!("key: {:?}, slot_size: {:?}", key, self.slot_size);
        let node = Node::deserialize(
            &self.buffer.borrow()[offset..offset + self.slot_size],
            self.callback_info_len,
        )
        .ok()?;
        Some(node)
    }

    fn write_node(&mut self, node: &Node, key: u32) -> Result<(), IoError> {
        let offset = SLAB_HEADER_LEN + (key as usize) * self.slot_size;
        node.serialize(&mut &mut self.buffer.borrow_mut()[offset..])
    }

    fn insert(&mut self, val: &Node) -> Result<u32, IoError> {
        if self.header.free_list_len == 0 {
            if self.header.bump_index as usize == self.capacity() as usize {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }

            if self.header.bump_index == std::u32::MAX as u64 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            let key = self.header.bump_index as u32;
            self.header.bump_index += 1;

            self.write_node(val, key)?;
            return Ok(key);
        }

        let key = self.header.free_list_head;
        let node = self.get_node(key).unwrap();

        let free_list_item = match node {
            Node::Free(f) => {
                assert!(self.header.free_list_len > 1);
                f
            }
            Node::LastFree(f) => {
                assert_eq!(self.header.free_list_len, 1);
                f
            }
            _ => unreachable!(),
        };

        let next_free_list_head = free_list_item.next;
        self.header.free_list_head = next_free_list_head;
        self.header.free_list_len -= 1;

        self.write_node(val, key).unwrap();
        Ok(key)
    }

    fn remove(&mut self, key: u32) -> Option<Node> {
        let val = self.get_node(key)?;
        let new_free_node = FreeNode {
            next: self.header.free_list_head,
        };
        let node = if self.header.free_list_len == 0 {
            Node::LastFree(new_free_node)
        } else {
            Node::Free(new_free_node)
        };

        self.write_node(&node, key).unwrap();

        self.header.free_list_head = key;
        self.header.free_list_len += 1;
        Some(val)
    }
}

// Critbit tree walks
impl<'a> Slab<'a> {
    pub fn root(&self) -> Option<NodeHandle> {
        if self.header.leaf_count == 0 {
            return None;
        }

        Some(self.header.root_node)
    }

    fn find_min_max(&self, find_max: bool) -> Option<NodeHandle> {
        let mut root: NodeHandle = self.root()?;
        loop {
            let root_contents = self.get_node(root).unwrap();
            match root_contents {
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
        new_leaf_node: &Node,
    ) -> Result<(NodeHandle, Option<Node>), AoError> {
        let new_leaf = new_leaf_node.as_leaf().unwrap();
        let mut root: NodeHandle = match self.root() {
            Some(h) => h,
            None => {
                // create a new root if none exists
                match self.insert(&new_leaf_node) {
                    Ok(handle) => {
                        self.header.root_node = handle;
                        self.header.leaf_count = 1;
                        return Ok((handle, None));
                    }
                    Err(_) => return Err(AoError::SlabOutOfSpace),
                }
            }
        };
        loop {
            // check if the new node will be a child of the root
            let root_contents = self.get_node(root).unwrap();
            let root_key = root_contents.key().unwrap();
            if root_key == new_leaf.key {
                if let Node::Leaf(_) = root_contents {
                    // clobber the existing leaf
                    self.write_node(&new_leaf_node, root).unwrap();
                    return Ok((root, Some(root_contents)));
                }
            }
            let shared_prefix_len: u32 = (root_key ^ new_leaf.key).leading_zeros();
            if let Node::Inner(ref inner) = root_contents {
                let keep_old_root = shared_prefix_len >= inner.prefix_len;
                if keep_old_root {
                    root = inner.walk_down(new_leaf.key).0;
                    continue;
                };
            }

            // change the root in place to represent the LCA of [new_leaf] and [root]
            let crit_bit_mask: u128 = (1u128 << 127) >> shared_prefix_len;
            let new_leaf_crit_bit = (crit_bit_mask & new_leaf.key) != 0;
            let old_root_crit_bit = !new_leaf_crit_bit;

            let new_leaf_handle = self
                .insert(&new_leaf_node)
                .map_err(|_| AoError::SlabOutOfSpace)?;
            let moved_root_handle = match self.insert(&root_contents) {
                Ok(h) => h,
                Err(_) => {
                    self.remove(new_leaf_handle).unwrap();
                    return Err(AoError::SlabOutOfSpace);
                }
            };

            let mut root_node = InnerNode {
                prefix_len: shared_prefix_len,
                key: new_leaf.key,
                children: [0; 2],
            };

            root_node.children[new_leaf_crit_bit as usize] = new_leaf_handle;
            root_node.children[old_root_crit_bit as usize] = moved_root_handle;

            self.write_node(&Node::Inner(root_node), root).unwrap();

            self.header.leaf_count += 1;
            return Ok((new_leaf_handle, None));
        }
    }

    pub fn remove_by_key(&mut self, search_key: u128) -> Option<Node> {
        let mut parent_h = self.root()?;
        let mut child_h;
        let mut crit_bit;
        let n = self.get_node(parent_h).unwrap();
        match n {
            Node::Leaf(ref leaf) if leaf.key == search_key => {
                assert_eq!(identity(self.header.leaf_count), 1);
                self.header.root_node = 0;
                self.header.leaf_count = 0;
                let _old_root = self.remove(parent_h).unwrap();
                return Some(n);
            }
            Node::Leaf(_) => return None,
            Node::Inner(inner) => {
                let (ch, cb) = inner.walk_down(search_key);
                child_h = ch;
                crit_bit = cb;
            }
            _ => unreachable!(),
        }
        loop {
            match self.get_node(child_h).unwrap() {
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
                _ => unreachable!(),
            }
        }
        // replace parent with its remaining child node
        // free child_h, replace *parent_h with *other_child_h, free other_child_h
        let other_child_h =
            self.get_node(parent_h).unwrap().children().unwrap()[!crit_bit as usize];
        let other_child_node_contents = self.remove(other_child_h).unwrap();
        self.write_node(&other_child_node_contents, parent_h)
            .unwrap();
        self.header.leaf_count -= 1;
        let removed_leaf = self.remove(child_h).unwrap();
        Some(removed_leaf)
    }

    pub fn remove_min(&mut self) -> Option<Node> {
        self.remove_by_key(self.get_node(self.find_min()?)?.key()?)
    }

    pub fn remove_max(&mut self) -> Option<Node> {
        self.remove_by_key(self.get_node(self.find_max()?)?.key()?)
    }

    /////////////////////////////////////////
    // Misc

    #[cfg(test)]
    fn find_by_key(&self, search_key: u128) -> Option<NodeHandle> {
        let mut node_handle: NodeHandle = self.root()?;
        loop {
            let node = self.get_node(node_handle).unwrap();
            let node_prefix_len = node.prefix_len().unwrap();
            let node_key = node.key().unwrap();
            let common_prefix_len = (search_key ^ node_key).leading_zeros();
            if common_prefix_len < node_prefix_len {
                return None;
            }
            match node {
                Node::Leaf(_) => break Some(node_handle),
                Node::Inner(inner) => {
                    let crit_bit_mask = (1u128 << 127) >> node_prefix_len;
                    let _search_key_crit_bit = (search_key & crit_bit_mask) != 0;
                    node_handle = inner.walk_down(search_key).0;
                    continue;
                }
                _ => unreachable!(),
            }
        }
    }

    #[cfg(test)]
    fn traverse(&self) -> Vec<Node> {
        fn walk_rec<'a>(slab: &'a Slab, sub_root: NodeHandle, buf: &mut Vec<Node>) {
            let n = slab.get_node(sub_root).unwrap();
            match n {
                Node::Leaf(_) => {
                    buf.push(n);
                }
                Node::Inner(inner) => {
                    walk_rec(slab, inner.children[0], buf);
                    walk_rec(slab, inner.children[1], buf);
                }
                _ => unreachable!(),
            }
        }

        let mut buf = Vec::with_capacity(self.header.leaf_count as usize);
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
        println!("Callback info length {:?}", self.callback_info_len);
        println!("Slot size {:?}", self.slot_size);
        println!("Header (parsed):");
        let mut header_data = Vec::new();
        println!("{:?}", self.header);
        self.header.serialize(&mut header_data).unwrap();

        println!("Header (raw):");
        hexdump::hexdump(&header_data);
        let mut offset = SLAB_HEADER_LEN;
        let mut key = 0;
        while offset + self.slot_size < self.buffer.borrow().len() {
            println!("Slot {:?}", key);
            let n = Node::deserialize(
                &self.buffer.borrow()[offset..offset + self.slot_size],
                self.callback_info_len,
            )
            .unwrap();
            println!("{:?}", n);

            hexdump::hexdump(&self.buffer.borrow()[offset..offset + self.slot_size]);
            key += 1;
            offset += self.slot_size;
        }
        // println!("Data:");
        // hexdump::hexdump(&self.buffer.borrow()[SLAB_HEADER_LEN..]);
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
            assert!(node.prefix_len().unwrap() > last_prefix_len);
            let node_key = node.key().unwrap();
            assert_eq!(
                last_crit_bit,
                (node_key & ((1u128 << 127) >> last_prefix_len)) != 0
            );
            let prefix_mask = (((((1u128) << 127) as i128) >> last_prefix_len) as u128) << 1;
            assert_eq!(last_prefix & prefix_mask, node.key().unwrap() & prefix_mask);
            if let Some([c0, c1]) = node.children() {
                check_rec(
                    slab,
                    *c0,
                    node.prefix_len().unwrap(),
                    node_key,
                    false,
                    count,
                );
                check_rec(slab, *c1, node.prefix_len().unwrap(), node_key, true, count);
            }
        }
        if let Some(root) = self.root() {
            count += 1;
            let node = self.get_node(root).unwrap();
            let node_key = node.key().unwrap();
            if let Some([c0, c1]) = node.children() {
                check_rec(
                    self,
                    *c0,
                    node.prefix_len().unwrap(),
                    node_key,
                    false,
                    &mut count,
                );
                check_rec(
                    self,
                    *c1,
                    node.prefix_len().unwrap(),
                    node_key,
                    true,
                    &mut count,
                );
            }
        }
        assert_eq!(
            count + self.header.free_list_len as u64,
            identity(self.header.bump_index)
        );

        let mut free_nodes_remaining = self.header.free_list_len;
        let mut next_free_node = self.header.free_list_head;
        loop {
            let contents;
            match free_nodes_remaining {
                0 => break,
                1 => {
                    contents = self.get_node(next_free_node).unwrap();
                    assert!(matches!(contents, Node::LastFree(_)));
                }
                _ => {
                    contents = self.get_node(next_free_node).unwrap();
                    assert!(matches!(contents, Node::Free(_)));
                }
            };
            let free_node = match contents {
                Node::LastFree(f) | Node::Free(f) => f,
                _ => unreachable!(),
            };
            next_free_node = free_node.next;
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
    fn test_node_serialization() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut bytes = [0u8; 100];
        let mut w: &mut [u8] = &mut bytes;
        let l = LeafNode::new(rng.gen(), rng.gen::<[u8; 32]>().to_vec(), rng.gen());
        l.serialize(&mut w).unwrap();
        let new_leaf = LeafNode::deserialize(&bytes, 32).unwrap();
        assert_eq!(l, new_leaf);
        let node = Node::Leaf(l);
        w = &mut bytes;
        node.serialize(&mut &mut w).unwrap();
        let new_node = Node::deserialize(&bytes, 32).unwrap();
        assert_eq!(node, new_node);
    }

    #[test]
    fn simulate_find_min() {
        use std::collections::BTreeMap;

        for trial in 0..10u64 {
            let mut bytes = vec![0u8; 80_000];
            let slab_data = Rc::new(RefCell::new(&mut bytes[..]));
            let mut slab = Slab {
                buffer: Rc::clone(&slab_data),
                callback_info_len: 32,
                slot_size: Slab::compute_slot_size(32),
                header: SlabHeader::deserialize(&mut (&slab_data.borrow() as &[u8])).unwrap(),
            };

            let mut model: BTreeMap<u128, Node> = BTreeMap::new();

            let mut all_keys = vec![];

            let mut rng = StdRng::seed_from_u64(trial);

            assert_eq!(slab.find_min(), None);
            assert_eq!(slab.find_max(), None);

            for i in 0..100 {
                let key = rng.gen();
                let owner = Pubkey::new_unique();
                let qty = rng.gen();
                let leaf = Node::Leaf(LeafNode::new(key, owner.to_bytes().to_vec(), qty));

                println!("key : {:x}", key);
                // println!("owner : {:?}", &owner.to_bytes());
                println!("{}", i);
                slab.insert_leaf(&leaf).unwrap();
                model.insert(key, leaf).ok_or(()).unwrap_err();
                all_keys.push(key);

                // test find_by_key
                let valid_search_key = *all_keys.choose(&mut rng).unwrap();
                let invalid_search_key = rng.gen();

                for &search_key in &[valid_search_key, invalid_search_key] {
                    let slab_value = slab.find_by_key(search_key).and_then(|x| slab.get_node(x));
                    let model_value = model.get(&search_key).cloned();
                    assert_eq!(slab_value, model_value);
                }

                // test find_min
                let slab_min = slab.get_node(slab.find_min().unwrap()).unwrap();
                let model_min = model.iter().next().unwrap().1;
                assert_eq!(&slab_min, model_min);

                // test find_max
                let slab_max = slab.get_node(slab.find_max().unwrap()).unwrap();
                let model_max = model.iter().next_back().unwrap().1;
                assert_eq!(&slab_max, model_max);
            }
        }
    }

    #[test]
    fn simulate_operations() {
        use rand::distributions::WeightedIndex;
        use std::collections::BTreeMap;

        let mut bytes = vec![0u8; 800_000];
        let slab_data = Rc::new(RefCell::new(&mut bytes[..]));
        let mut slab = Slab {
            buffer: Rc::clone(&slab_data),
            callback_info_len: 32,
            slot_size: Slab::compute_slot_size(32),
            header: SlabHeader::deserialize(&mut (&slab_data.borrow() as &[u8])).unwrap(),
        };
        let mut model: BTreeMap<u128, Node> = BTreeMap::new();

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
                let slab_state: Vec<Node> = slab.traverse();
                assert_eq!(model_state, slab_state.iter().collect::<Vec<&Node>>());

                match weights[dist.sample(&mut rng)].0 {
                    op @ Op::InsertNew | op @ Op::InsertDup => {
                        let key = match op {
                            Op::InsertNew => rng.gen(),
                            Op::InsertDup => *all_keys.choose(&mut rng).unwrap(),
                            _ => unreachable!(),
                        };
                        let owner = Pubkey::new_unique();
                        let qty = rng.gen();
                        let leaf = Node::Leaf(LeafNode::new(key, owner.to_bytes().to_vec(), qty));

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
                            assert_eq!(identity(slab.header.leaf_count), 0);
                        } else {
                            let slab_min = slab.get_node(slab.find_min().unwrap()).unwrap();
                            let model_min = model.iter().next().unwrap().1;
                            assert_eq!(&slab_min, model_min);
                        }
                    }
                    Op::Max => {
                        if model.is_empty() {
                            assert_eq!(identity(slab.header.leaf_count), 0);
                        } else {
                            let slab_max = slab.get_node(slab.find_max().unwrap()).unwrap();
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
