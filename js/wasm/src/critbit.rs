use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::{FromPrimitive, ToPrimitive};
use std::convert::TryInto;
use std::io::Write;
use std::{cell::RefCell, convert::identity, rc::Rc};
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

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, PartialEq, FromPrimitive, ToPrimitive)]
#[repr(u8)]
#[allow(missing_docs)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum AccountTag {
    Uninitialized,
    Market,
    EventQueue,
    Bids,
    Asks,
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
    pub base_quantity: u64,
}

impl LeafNode {
    pub fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), IoError> {
        writer.write_all(&self.key.to_le_bytes())?;
        writer.write_all(&self.callback_info)?;
        writer.write_all(&self.base_quantity.to_le_bytes())?;
        Ok(())
    }

    pub fn deserialize(buf: &[u8], callback_info_len: usize) -> Result<Self, IoError> {
        let key = u128::from_le_bytes(
            buf[..16]
                .try_into()
                .map_err(|_| std::io::ErrorKind::InvalidData)?,
        );
        let callback_info = buf[16..callback_info_len + 16].to_owned();
        let base_quantity = u64::from_le_bytes(
            buf[callback_info_len + 16..callback_info_len + 24]
                .try_into()
                .map_err(|_| std::io::ErrorKind::InvalidData)?,
        );
        Ok(Self {
            key,
            callback_info,
            base_quantity,
        })
    }
}

pub const INNER_NODE_SIZE: usize = 32;

impl LeafNode {
    pub fn new(key: u128, callback_info: Vec<u8>, quantity: u64) -> Self {
        LeafNode {
            key,
            callback_info,
            base_quantity: quantity,
        }
    }

    pub fn price(&self) -> u64 {
        (self.key >> 64) as u64
    }

    pub fn order_id(&self) -> u128 {
        self.key
    }

    pub fn set_base_quantity(&mut self, quantity: u64) {
        self.base_quantity = quantity;
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
    market_address: [u8; 32],
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

    pub fn new(
        buffer: Rc<RefCell<&'a mut [u8]>>,
        callback_info_len: usize,
        slot_size: usize,
    ) -> Self {
        Self {
            header: SlabHeader::deserialize(&mut (&buffer.borrow() as &[u8])).unwrap(),
            buffer: Rc::clone(&buffer),
            callback_info_len,
            slot_size,
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

    pub fn find_node_sequence(&self, depth: usize, increasing: bool) -> Vec<NodeHandle> {
        let root = self.root();
        if root.is_none() {
            return vec![];
        }
        let mut result = Vec::with_capacity(depth);
        let mut search_stack = vec![root.unwrap()];
        while result.len() != depth {
            let current = search_stack.pop();
            if current.is_none() {
                break;
            }
            let node = self.get_node(current.unwrap()).unwrap();
            if let Node::Inner(ref inner) = node {
                search_stack.push(inner.children[increasing as usize]);
                search_stack.push(inner.children[1 - increasing as usize]);
            } else {
                result.push(current.unwrap());
            }
        }
        result
    }

    pub fn find_l2_depth(&self, depth: usize, increasing: bool) -> Vec<u64> {
        let root = self.root();
        if root.is_none() {
            return vec![];
        }
        let mut result = Vec::with_capacity(2 * depth);
        let mut search_stack = vec![root.unwrap()];
        while result.len() != 2 * depth {
            let current = search_stack.pop();
            if current.is_none() {
                break;
            }
            let node = self.get_node(current.unwrap()).unwrap();
            match node {
                Node::Inner(ref inner) => {
                    search_stack.push(inner.children[increasing as usize]);
                    search_stack.push(inner.children[1 - increasing as usize]);
                }
                Node::Leaf(ref leaf) => {
                    let leaf_price = leaf.price();
                    if result.last().map(|p| p == &leaf_price).unwrap_or(false) {
                        let idx = result.len() - 2;
                        result[idx] += leaf.base_quantity;
                    } else {
                        result.push(leaf.base_quantity);
                        result.push(leaf_price);
                    }
                }
                _ => unreachable!(),
            }
        }
        result
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
