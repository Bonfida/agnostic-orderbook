use crate::error::AoError;
use crate::state::{AccountTag, Side};
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{try_from_bytes, try_from_bytes_mut, Pod, Zeroable};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use std::cell::{Ref, RefMut};
use std::convert::TryInto;
use std::{cell::RefCell, convert::identity, rc::Rc};
// A Slab contains the data for a slab header and an array of nodes of a critbit tree
// whose leafs contain the data referencing an order of the orderbook.

////////////////////////////////////
// Nodes

#[doc(hidden)]
pub type NodeHandle = u32;

#[doc(hidden)]
pub type IoError = std::io::Error;
#[doc(hidden)]
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct InnerNode {
    key: u128,
    prefix_len: u64,
    pub children: [u32; 2],
}

impl InnerNode {
    fn walk_down(&self, search_key: u128) -> (NodeHandle, bool) {
        let crit_bit_mask = (1u128 << 127) >> self.prefix_len;
        let crit_bit = (search_key & crit_bit_mask) != 0;
        (self.children[crit_bit as usize], crit_bit)
    }
}

/// A critibit leaf node
#[derive(Debug, PartialEq, PartialOrd, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct LeafNode {
    /// The key is the associated order id
    pub key: u128,
    /// A pointer into the underlying Slab to retrieve the node's associated callback info. The [`Slab::get_callback_info`] method can be used.
    pub callback_info_pt: u64,
    /// The quantity of base asset associated with the underlying order
    pub base_quantity: u64,
}

pub(crate) const NODE_SIZE: usize = 32;
pub(crate) const FREE_NODE_SIZE: usize = 4;

pub(crate) const NODE_TAG_SIZE: usize = 8;

/// The size in bytes of a critbit slot
pub const SLOT_SIZE: usize = NODE_TAG_SIZE + NODE_SIZE;

impl LeafNode {
    /// Parse a leaf node's price
    pub fn price(&self) -> u64 {
        (self.key >> 64) as u64
    }

    /// Get the associated order id
    pub fn order_id(&self) -> u128 {
        self.key
    }

    pub(crate) fn set_base_quantity(&mut self, quantity: u64) {
        self.base_quantity = quantity;
    }
}

#[doc(hidden)]
#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct FreeNode {
    next: u32,
}

#[derive(Debug, PartialEq, Clone, FromPrimitive)]
pub(crate) enum NodeTag {
    Uninitialized,
    Inner,
    Leaf,
    Free,
    LastFree,
}

#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Uninitialized,
    Inner(InnerNode),
    Leaf(LeafNode),
    Free(FreeNode),
    LastFree(FreeNode),
}

#[doc(hidden)]
pub enum NodeRef<'a> {
    Uninitialized,
    Inner(Ref<'a, InnerNode>),
    Leaf(Ref<'a, LeafNode>),
    Free(Ref<'a, FreeNode>),
    LastFree(Ref<'a, FreeNode>),
}

pub(crate) enum NodeRefMut<'a> {
    Uninitialized,
    Inner(RefMut<'a, InnerNode>),
    Leaf(RefMut<'a, LeafNode>),
    Free(RefMut<'a, FreeNode>),
    LastFree(RefMut<'a, FreeNode>),
}

impl<'a> Node {
    pub fn as_leaf(&self) -> Option<&LeafNode> {
        match &self {
            Node::Leaf(leaf_ref) => Some(leaf_ref),
            _ => None,
        }
    }

    pub(crate) fn tag(&self) -> NodeTag {
        match self {
            Node::Uninitialized => NodeTag::Uninitialized,
            Node::Inner(_) => NodeTag::Inner,
            Node::Leaf(_) => NodeTag::Leaf,
            Node::Free(_) => NodeTag::Free,
            Node::LastFree(_) => NodeTag::LastFree,
        }
    }
}

impl<'a> NodeRef<'a> {
    fn key(&self) -> Option<u128> {
        match &self {
            Self::Inner(inner) => Some(inner.key),
            Self::Leaf(leaf) => Some(leaf.key),
            _ => None,
        }
    }

    #[cfg(any(test, feature = "utils"))]
    fn prefix_len(&self) -> Result<u64, IoError> {
        match &self {
            Self::Inner(i) => Ok(i.prefix_len),
            Self::Leaf(_) => Ok(128),
            _ => Err(std::io::ErrorKind::InvalidData.into()),
        }
    }

    fn children(&self) -> Option<Ref<'a, [u32; 2]>> {
        match &self {
            Self::Inner(i) => Some(Ref::map(Ref::clone(i), |k| &k.children)),
            _ => None,
        }
    }

    pub fn as_leaf(&self) -> Option<Ref<'a, LeafNode>> {
        match &self {
            Self::Leaf(leaf_ref) => Some(Ref::clone(leaf_ref)),
            _ => None,
        }
    }

    pub fn to_owned(&self) -> Node {
        match &self {
            NodeRef::Uninitialized => Node::Uninitialized,
            NodeRef::Inner(n) => Node::Inner(**n),
            NodeRef::Leaf(n) => Node::Leaf(**n),
            NodeRef::Free(n) => Node::Free(**n),
            NodeRef::LastFree(n) => Node::LastFree(**n),
        }
    }
}

////////////////////////////////////
// Slabs

#[doc(hidden)]
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct SlabHeader {
    pub account_tag: AccountTag,
    pub bump_index: u64,
    pub free_list_len: u64,
    pub free_list_head: u32,
    pub callback_memory_offset: u64,
    pub callback_free_list_len: u64,
    pub callback_free_list_head: u64,
    pub callback_bump_index: u64,

    pub root_node: u32,
    pub leaf_count: u64,
    pub market_address: Pubkey,
}
#[doc(hidden)]
pub const SLAB_HEADER_LEN: usize = 97;
#[doc(hidden)]
pub const PADDED_SLAB_HEADER_LEN: usize = SLAB_HEADER_LEN + 7;

/// A Slab contains the data for a slab header and an array of nodes of a critbit tree
/// whose leafs contain the data referencing an order of the orderbook.
#[derive(Clone)]
pub struct Slab<'a> {
    #[doc(hidden)]
    pub header: SlabHeader,
    /// The underlying account data
    pub buffer: Rc<RefCell<&'a mut [u8]>>,
    #[doc(hidden)]
    pub callback_info_len: usize,
}

// Data access methods
impl<'a> Slab<'a> {
    pub(crate) fn check(&self, side: Side) -> bool {
        match side {
            Side::Bid => self.header.account_tag == AccountTag::Bids,
            Side::Ask => self.header.account_tag == AccountTag::Asks,
        }
    }
    /// Intialize a Slab object from an AccountInfo
    pub fn new_from_acc_info(acc_info: &AccountInfo<'a>, callback_info_len: usize) -> Self {
        // assert_eq!(len_without_header % slot_size, 0);
        Self {
            buffer: Rc::clone(&acc_info.data),
            callback_info_len,
            header: SlabHeader::deserialize(&mut (&acc_info.data.borrow() as &[u8])).unwrap(),
        }
    }
    /// Intialize a Slab object from a wrapped buffer
    pub fn new(buffer: Rc<RefCell<&'a mut [u8]>>, callback_info_len: usize) -> Self {
        Self {
            header: SlabHeader::deserialize(&mut (&buffer.borrow() as &[u8])).unwrap(),
            buffer: Rc::clone(&buffer),
            callback_info_len,
        }
    }

    /// Instantiate a Slab object directly from a buffer reference
    pub fn from_bytes(buffer: &'a mut [u8], callback_info_len: usize) -> Self {
        Self {
            header: SlabHeader::deserialize(&mut (buffer as &[u8])).unwrap(),
            buffer: Rc::new(RefCell::new(buffer)),
            callback_info_len,
        }
    }

    pub(crate) fn write_header(&self) {
        self.header
            .serialize(&mut &mut self.buffer.borrow_mut()[..SLAB_HEADER_LEN])
            .unwrap()
    }

    #[doc(hidden)]
    pub fn initialize(
        mut bids_account: &mut [u8],
        mut asks_account: &mut [u8],
        market_address: Pubkey,
        callback_info_len: usize,
    ) {
        let order_capacity =
            (asks_account.len() - PADDED_SLAB_HEADER_LEN) / (SLOT_SIZE * 2 + callback_info_len);

        let mut header = SlabHeader {
            account_tag: AccountTag::Asks,
            bump_index: 0,
            free_list_len: 0,
            free_list_head: 0,
            root_node: 0,
            leaf_count: 0,
            market_address,
            callback_memory_offset: asks_callback_memory_offset as u64,
            callback_bump_index: asks_callback_memory_offset as u64,
            callback_free_list_head: 0,
            callback_free_list_len: 0,
        };
        header.serialize(&mut (asks_account)).unwrap();

        let bids_order_capacity =
            (bids_account.len() - PADDED_SLAB_HEADER_LEN) / (SLOT_SIZE * 2 + callback_info_len);
        let bids_callback_memory_offset =
            Slab::compute_callback_memory_offset(bids_order_capacity as usize);

        header.account_tag = AccountTag::Bids;
        header.callback_memory_offset = bids_callback_memory_offset as u64;
        header.callback_bump_index = bids_callback_memory_offset as u64;
        header.serialize(&mut (bids_account)).unwrap();
    }

    /// Compute the allocation size for an orderbook Slab of a desired capacity
    pub fn compute_allocation_size(
        desired_order_capacity: usize,
        callback_info_len: usize,
    ) -> usize {
        PADDED_SLAB_HEADER_LEN + desired_order_capacity * (2 * SLOT_SIZE + callback_info_len)
    }
}

// Tree nodes manipulation methods
impl<'a> Slab<'a> {
    fn capacity(&self) -> u64 {
        Self::compute_capacity(self.callback_info_len, self.buffer.borrow().len())
    }

    fn compute_capacity(callback_info_len: usize, account_length: usize) -> u64 {
        let root_size = SLOT_SIZE + callback_info_len;
        ((account_length - PADDED_SLAB_HEADER_LEN - root_size)
            / (2 * SLOT_SIZE + callback_info_len)) as u64
            + 1
    }

    fn compute_callback_memory_offset(order_capacity: usize) -> usize {
        PADDED_SLAB_HEADER_LEN + (2 * order_capacity - 1) * SLOT_SIZE
    }

    #[doc(hidden)]
    pub fn get_node(&self, key: u32) -> Option<NodeRef> {
        let mut offset = PADDED_SLAB_HEADER_LEN + (key as usize) * SLOT_SIZE;
        // println!("key: {:?}, slot_size: {:?}", key, self.slot_size);
        let node_tag = NodeTag::from_u64(u64::from_le_bytes(
            self.buffer.borrow()[offset..offset + NODE_TAG_SIZE]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        offset += NODE_TAG_SIZE;
        let node = match node_tag {
            NodeTag::Leaf => {
                let node: Ref<LeafNode> = Ref::map(self.buffer.borrow(), |s| {
                    try_from_bytes(&s[offset..offset + NODE_SIZE]).unwrap()
                });
                NodeRef::Leaf(node)
            }
            NodeTag::Inner => {
                let node: Ref<InnerNode> = Ref::map(self.buffer.borrow(), |s| {
                    try_from_bytes(&s[offset..offset + NODE_SIZE]).unwrap()
                });
                NodeRef::Inner(node)
            }
            NodeTag::Free | NodeTag::LastFree => {
                let node: Ref<FreeNode> = Ref::map(self.buffer.borrow(), |s| {
                    try_from_bytes(&s[offset..offset + FREE_NODE_SIZE]).unwrap()
                });
                match node_tag {
                    NodeTag::Free => NodeRef::Free(node),
                    NodeTag::LastFree => NodeRef::LastFree(node),
                    _ => unreachable!(),
                }
            }
            NodeTag::Uninitialized => NodeRef::Uninitialized,
        };
        Some(node)
    }

    pub(crate) fn get_node_mut(&self, key: u32) -> Option<NodeRefMut> {
        let mut offset = PADDED_SLAB_HEADER_LEN + (key as usize) * SLOT_SIZE;
        // println!("key: {:?}, slot_size: {:?}", key, self.slot_size);
        let node_tag = NodeTag::from_u64(u64::from_le_bytes(
            self.buffer.borrow()[offset..offset + NODE_TAG_SIZE]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        offset += NODE_TAG_SIZE;
        let node = match node_tag {
            NodeTag::Leaf => {
                let node: RefMut<LeafNode> = RefMut::map(self.buffer.borrow_mut(), |s| {
                    try_from_bytes_mut(&mut s[offset..offset + NODE_SIZE]).unwrap()
                });
                NodeRefMut::Leaf(node)
            }
            NodeTag::Inner => {
                let node: RefMut<InnerNode> = RefMut::map(self.buffer.borrow_mut(), |s| {
                    try_from_bytes_mut(&mut s[offset..offset + NODE_SIZE]).unwrap()
                });
                NodeRefMut::Inner(node)
            }
            NodeTag::Free | NodeTag::LastFree => {
                let node: RefMut<FreeNode> = RefMut::map(self.buffer.borrow_mut(), |s| {
                    try_from_bytes_mut(&mut s[offset..offset + FREE_NODE_SIZE]).unwrap()
                });
                match node_tag {
                    NodeTag::Free => NodeRefMut::Free(node),
                    NodeTag::LastFree => NodeRefMut::LastFree(node),
                    _ => unreachable!(),
                }
            }
            NodeTag::Uninitialized => NodeRefMut::Uninitialized,
        };
        Some(node)
    }

    fn allocate(&mut self, node_type: &NodeTag) -> Result<u32, IoError> {
        if self.header.free_list_len == 0 {
            if self.header.bump_index as usize == (2 * self.capacity() - 1) as usize {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }

            if self.header.bump_index == std::u32::MAX as u64 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            let key = self.header.bump_index;
            let offset = PADDED_SLAB_HEADER_LEN + (key as usize) * SLOT_SIZE;
            self.header.bump_index += 1;
            match node_type {
                NodeTag::Inner => {
                    *try_from_bytes_mut(&mut self.buffer.borrow_mut()[offset..offset + 8])
                        .unwrap() = NodeTag::Inner as u64;
                    #[cfg(feature = "debug-asserts")]
                    assert_eq!(self.buffer.borrow()[offset], NodeTag::Inner as u8);
                }
                NodeTag::Leaf => {
                    *try_from_bytes_mut(&mut self.buffer.borrow_mut()[offset..offset + 8])
                        .unwrap() = NodeTag::Leaf as u64;
                    #[cfg(feature = "debug-asserts")]
                    assert_eq!(self.buffer.borrow()[offset], NodeTag::Leaf as u8);
                }
                _ => panic!(),
            }
            return Ok(key as u32);
        }

        let key = self.header.free_list_head;
        #[cfg(feature = "debug-asserts")]
        {
            let node = self.get_node(key).unwrap();

            match node {
                NodeRef::Free(_) => {
                    assert!(self.header.free_list_len > 1);
                }
                NodeRef::LastFree(_) => {
                    assert_eq!(self.header.free_list_len, 1);
                }
                _ => unreachable!(),
            };
        }

        let next_free_list_head = {
            let key = self.header.free_list_head;
            let node = self.get_node(key).unwrap();

            let free_list_item = match node {
                NodeRef::Free(f) => {
                    assert!(self.header.free_list_len > 1);
                    f
                }
                NodeRef::LastFree(f) => {
                    assert_eq!(self.header.free_list_len, 1);
                    f
                }
                _ => unreachable!(),
            };
            free_list_item.next
        };

        let offset = PADDED_SLAB_HEADER_LEN + (key as usize) * SLOT_SIZE;
        match node_type {
            NodeTag::Inner => {
                *try_from_bytes_mut(&mut self.buffer.borrow_mut()[offset..offset + 8]).unwrap() =
                    NodeTag::Inner as u64;
            }
            NodeTag::Leaf => {
                *try_from_bytes_mut(&mut self.buffer.borrow_mut()[offset..offset + 8]).unwrap() =
                    NodeTag::Leaf as u64;
            }
            _ => panic!(),
        }
        self.header.free_list_head = next_free_list_head;
        self.header.free_list_len -= 1;
        Ok(key)
    }

    fn remove(&mut self, key: u32) {
        let offset = PADDED_SLAB_HEADER_LEN + (key as usize) * SLOT_SIZE;
        let old_tag = NodeTag::from_u64(u64::from_le_bytes(
            self.buffer.borrow()[offset..offset + NODE_TAG_SIZE]
                .try_into()
                .unwrap(),
        ))
        .unwrap();
        if old_tag == NodeTag::Leaf {
            let callback_info_index = self
                .get_node(key)
                .unwrap()
                .as_leaf()
                .unwrap()
                .callback_info_pt;
            self.clear_callback_info(callback_info_index as usize);
        }
        let new_tag = if self.header.free_list_len == 0 {
            NodeTag::LastFree
        } else {
            NodeTag::Free
        };
        *try_from_bytes_mut(&mut self.buffer.borrow_mut()[offset..offset + 8]).unwrap() =
            new_tag as u64;
        if let NodeRefMut::Free(mut new_free_node) = self.get_node_mut(key).unwrap() {
            new_free_node.next = self.header.free_list_head
        };

        self.header.free_list_head = key;
        self.header.free_list_len += 1;
    }

    fn insert_node(&mut self, node: &Node) -> Result<u32, IoError> {
        let handle = self.allocate(&node.tag())?;
        self.write_node(node, handle);
        Ok(handle)
    }

    pub(crate) fn write_callback_info(&mut self, callback_info: &[u8]) -> Result<u64, IoError> {
        let h = if self.header.callback_free_list_len > 0 {
            let next_free_spot = u64::from_le_bytes(
                self.buffer.borrow()[self.header.callback_free_list_head as usize
                    ..self.header.callback_free_list_head as usize + 8]
                    .try_into()
                    .unwrap(),
            );
            let h = self.header.callback_free_list_head;
            self.header.callback_free_list_head = next_free_spot;
            self.header.callback_free_list_len -= 1;
            h as usize
        } else {
            let h = self.header.callback_bump_index;
            self.header.callback_bump_index += self.callback_info_len as u64;
            h as usize
        };
        self.buffer
            .borrow_mut()
            .get_mut(h..h + self.callback_info_len)
            .map(|s| s.copy_from_slice(callback_info))
            .ok_or(std::io::ErrorKind::UnexpectedEof)?;
        Ok(h as u64)
    }

    fn clear_callback_info(&mut self, callback_info_pt: usize) {
        self.buffer.borrow_mut()[callback_info_pt..callback_info_pt + 8]
            .copy_from_slice(&self.header.callback_free_list_head.to_le_bytes());
        self.header.callback_free_list_head = callback_info_pt as u64;
        self.header.callback_free_list_len += 1;
    }

    /// Retrieve a callback info slice
    pub fn get_callback_info(&self, callback_info_pt: usize) -> Ref<[u8]> {
        Ref::map(self.buffer.borrow(), |r| {
            &r[callback_info_pt..callback_info_pt + self.callback_info_len]
        })
    }

    pub(crate) fn write_node(&mut self, node: &Node, handle: NodeHandle) {
        match (node, self.get_node_mut(handle)) {
            (Node::Inner(i), Some(NodeRefMut::Inner(mut r))) => {
                *r = *i;
            }
            (Node::Leaf(l), Some(NodeRefMut::Leaf(mut r))) => {
                *r = *l;
            }
            _ => unreachable!(),
        }
    }
}

// Critbit tree walks
impl<'a> Slab<'a> {
    #[doc(hidden)]
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
                NodeRef::Inner(i) => {
                    root = i.children[if find_max { 1 } else { 0 }];
                    continue;
                }
                _ => return Some(root),
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

    pub(crate) fn insert_leaf(
        &mut self,
        new_leaf_node: &Node,
    ) -> Result<(NodeHandle, Option<Node>), AoError> {
        let new_leaf = new_leaf_node.as_leaf().unwrap();
        let mut root: NodeHandle = match self.root() {
            Some(h) => h,
            None => {
                // create a new root if none exists
                let new_leaf_key = self
                    .insert_node(new_leaf_node)
                    .map_err(|_| AoError::SlabOutOfSpace)?;
                self.header.root_node = new_leaf_key;
                self.header.leaf_count += 1;
                return Ok((new_leaf_key, None));
            }
        };
        let mut parent_node: Option<NodeHandle> = None;
        let mut previous_critbit: Option<bool> = None;
        loop {
            // check if the new node will be a child of the root
            let root_contents = self.get_node(root).unwrap();
            let root_key = root_contents.key().unwrap();
            if root_key == new_leaf.key {
                if let NodeRef::Leaf(l) = root_contents {
                    // clobber the existing leaf
                    let root_leaf_copy = *l;
                    drop(l);
                    if let NodeRefMut::Leaf(mut root_leaf) = self.get_node_mut(root).unwrap() {
                        *root_leaf = *new_leaf;
                    };
                    return Ok((root, Some(Node::Leaf(root_leaf_copy))));
                }
            }
            let shared_prefix_len: u32 = (root_key ^ new_leaf.key).leading_zeros();
            if let NodeRef::Inner(ref inner) = root_contents {
                let keep_old_root = shared_prefix_len >= inner.prefix_len as u32;
                if keep_old_root {
                    parent_node = Some(root);
                    let r = inner.walk_down(new_leaf.key);
                    root = r.0;
                    previous_critbit = Some(r.1);
                    continue;
                };
            }

            // change the root in place to represent the LCA of [new_leaf] and [root]
            let crit_bit_mask: u128 = (1u128 << 127) >> shared_prefix_len;
            let new_leaf_crit_bit = (crit_bit_mask & new_leaf.key) != 0;
            let old_root_crit_bit = !new_leaf_crit_bit;

            drop(root_contents);

            // Write new leaf to slab
            let new_leaf_handle = self
                .insert_node(new_leaf_node)
                .map_err(|_| AoError::SlabOutOfSpace)?;

            let new_root_node_handle = self.allocate(&NodeTag::Inner).map_err(|_| {
                // Prevent potential leak from previous allocation
                self.remove(new_leaf_handle);
                AoError::SlabOutOfSpace
            })?;

            if let NodeRefMut::Inner(mut i) = self.get_node_mut(new_root_node_handle).unwrap() {
                i.prefix_len = shared_prefix_len as u64;
                i.key = new_leaf.key;
                i.children[new_leaf_crit_bit as usize] = new_leaf_handle;
                i.children[old_root_crit_bit as usize] = root;
            } else {
                unreachable!()
            }

            if let Some(NodeRefMut::Inner(mut i)) =
                parent_node.map(|k| self.get_node_mut(k).unwrap())
            {
                i.children[previous_critbit.unwrap() as usize] = new_root_node_handle;
            }
            // Split condition here works around borrow checker
            if parent_node.is_none() {
                self.header.root_node = new_root_node_handle;
            }

            self.header.leaf_count += 1;
            return Ok((new_leaf_handle, None));
        }
    }

    /// This function corrupts the node's callback information when erasing it!
    pub fn remove_by_key(&mut self, search_key: u128) -> Option<(Node, Vec<u8>)> {
        let mut grandparent_h: Option<NodeHandle> = None;
        let mut parent_h = self.root()?;
        // We have to initialize the values to work around the type checker
        let mut child_h = 0;
        let mut crit_bit = false;
        let mut prev_crit_bit: Option<bool> = None;
        let mut remove_root = None;
        {
            let n = self.get_node(parent_h).unwrap();
            match n {
                NodeRef::Leaf(leaf) if leaf.key == search_key => {
                    assert_eq!(identity(self.header.leaf_count), 1);
                    let leaf_copy = Node::Leaf(*leaf);
                    drop(leaf);
                    remove_root = Some(leaf_copy);
                }
                NodeRef::Leaf(_) => return None,
                NodeRef::Inner(inner) => {
                    let (ch, cb) = inner.walk_down(search_key);
                    child_h = ch;
                    crit_bit = cb;
                }
                _ => unreachable!(),
            }
        }
        if let Some(leaf_copy) = remove_root {
            let callback_info = self
                .get_callback_info(leaf_copy.as_leaf().unwrap().callback_info_pt as usize)
                .to_vec();
            self.remove(parent_h);

            self.header.root_node = 0;
            self.header.leaf_count = 0;
            return Some((leaf_copy, callback_info));
        }
        loop {
            match self.get_node(child_h).unwrap() {
                NodeRef::Inner(inner) => {
                    let (grandchild_h, grandchild_crit_bit) = inner.walk_down(search_key);
                    grandparent_h = Some(parent_h);
                    parent_h = child_h;
                    child_h = grandchild_h;
                    prev_crit_bit = Some(crit_bit);
                    crit_bit = grandchild_crit_bit;
                    continue;
                }
                NodeRef::Leaf(leaf) => {
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

        if let Some(NodeRefMut::Inner(mut r)) = grandparent_h.map(|h| self.get_node_mut(h).unwrap())
        {
            r.children[prev_crit_bit.unwrap() as usize] = other_child_h
        }
        // Split condition here works around borrow checker
        if grandparent_h.is_none() {
            self.header.root_node = other_child_h;
        }
        self.header.leaf_count -= 1;
        let removed_leaf = self.get_node(child_h).unwrap().to_owned();
        let callback_info = self
            .get_callback_info(removed_leaf.as_leaf().unwrap().callback_info_pt as usize)
            .to_vec();
        self.remove(child_h);
        self.remove(parent_h);
        Some((removed_leaf, callback_info))
    }

    pub(crate) fn remove_min(&mut self) -> Option<(Node, Vec<u8>)> {
        let key = self.get_node(self.find_min()?)?.key()?;
        self.remove_by_key(key)
    }

    pub(crate) fn remove_max(&mut self) -> Option<(Node, Vec<u8>)> {
        let key = self.get_node(self.find_max()?)?.key()?;
        self.remove_by_key(key)
    }

    /////////////////////////////////////////
    // Misc

    #[cfg(any(test, feature = "utils"))]
    pub fn find_by_key(&self, search_key: u128) -> Option<NodeHandle> {
        let mut node_handle: NodeHandle = self.root()?;
        loop {
            let node = self.get_node(node_handle).unwrap();
            let node_prefix_len = node.prefix_len().unwrap();
            let node_key = node.key().unwrap();
            let common_prefix_len = (search_key ^ node_key).leading_zeros();
            if common_prefix_len < node_prefix_len as u32 {
                return None;
            }
            match node {
                NodeRef::Leaf(_) => break Some(node_handle),
                NodeRef::Inner(inner) => {
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
    fn traverse<T: CallbackInfo>(&self) -> Vec<(Node, T)> {
        fn walk_rec<'a, S: CallbackInfo>(
            slab: &'a Slab,
            sub_root: NodeHandle,
            buf: &mut Vec<(Node, S)>,
        ) {
            let n = slab.get_node(sub_root).unwrap().to_owned();
            match n {
                Node::Leaf(ref l) => {
                    let callback_info =
                        S::from_bytes(&slab.get_callback_info(l.callback_info_pt as usize));
                    buf.push((n, callback_info));
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
        println!("Slot size {:?}", SLOT_SIZE);
        println!("Header (parsed):");
        let mut header_data = Vec::new();
        println!("{:?}", self.header);
        self.header.serialize(&mut header_data).unwrap();

        println!("Header (raw):");
        hexdump::hexdump(&header_data);
        let mut offset = PADDED_SLAB_HEADER_LEN;
        let mut key = 0;
        while offset + SLOT_SIZE < self.buffer.borrow().len() {
            println!("Slot {:?}", key);
            let n = self.get_node(key).unwrap().to_owned();
            println!("{:?}", n);

            hexdump::hexdump(&self.buffer.borrow()[offset..offset + SLOT_SIZE]);
            key += 1;
            offset += SLOT_SIZE;
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
            last_prefix_len: u64,
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
            if let Some(c) = node.children() {
                check_rec(
                    slab,
                    c[0],
                    node.prefix_len().unwrap(),
                    node_key,
                    false,
                    count,
                );
                check_rec(
                    slab,
                    c[1],
                    node.prefix_len().unwrap(),
                    node_key,
                    true,
                    count,
                );
            }
        }
        if let Some(root) = self.root() {
            count += 1;
            let node = self.get_node(root).unwrap();
            let node_key = node.key().unwrap();
            if let Some(c) = node.children() {
                check_rec(
                    self,
                    c[0],
                    node.prefix_len().unwrap(),
                    node_key,
                    false,
                    &mut count,
                );
                check_rec(
                    self,
                    c[1],
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
                    assert!(matches!(contents, NodeRef::LastFree(_)));
                }
                _ => {
                    contents = self.get_node(next_free_node).unwrap();
                    assert!(matches!(contents, NodeRef::Free(_)));
                }
            };
            let free_node = match contents {
                NodeRef::LastFree(f) | NodeRef::Free(f) => f,
                _ => unreachable!(),
            };
            next_free_node = free_node.next;
            free_nodes_remaining -= 1;
        }
    }
}

trait CallbackInfo: Sized {
    fn from_bytes(data: &[u8]) -> Self;
}

impl CallbackInfo for Pubkey {
    fn from_bytes(data: &[u8]) -> Self {
        Self::new(data)
    }
}

impl<'slab> Slab<'slab> {
    /// Get a price ascending or price descending iterator over all the Slab's orders
    pub fn into_iter(self, price_ascending: bool) -> impl Iterator<Item = LeafNode> + 'slab {
        SlabIterator {
            search_stack: self.root().iter().copied().collect(),
            slab: self,
            ascending: price_ascending,
        }
    }
}

struct SlabIterator<'slab> {
    slab: Slab<'slab>,
    search_stack: Vec<u32>,
    ascending: bool,
}

impl<'slab> Iterator for SlabIterator<'slab> {
    type Item = LeafNode;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(current) = self.search_stack.pop() {
            match self.slab.get_node(current) {
                Some(NodeRef::Inner(i)) => {
                    self.search_stack.push(i.children[self.ascending as usize]);
                    self.search_stack.push(i.children[!self.ascending as usize]);
                }
                Some(NodeRef::Leaf(l)) => return Some(*l),
                _ => unreachable!(),
            }
        }
        None
    }
}

/////////////////////////////////////
// Tests

#[cfg(test)]
mod tests {

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

        for trial in 0..10u64 {
            let mut bytes = vec![0u8; 80_000];
            let order_capacity = (bytes.len() - PADDED_SLAB_HEADER_LEN) / (SLOT_SIZE * 2 + 32);
            let slab_data = Rc::new(RefCell::new(&mut bytes[..]));

            let callback_memory_offset = PADDED_SLAB_HEADER_LEN + 2 * order_capacity * SLOT_SIZE;
            let mut slab = Slab {
                buffer: Rc::clone(&slab_data),
                callback_info_len: 32,
                header: SlabHeader {
                    account_tag: AccountTag::Asks,
                    bump_index: 0,
                    free_list_len: 0,
                    free_list_head: 0,
                    callback_memory_offset: callback_memory_offset as u64,
                    callback_free_list_len: 0,
                    callback_free_list_head: 0,
                    callback_bump_index: callback_memory_offset as u64,
                    root_node: 0,
                    leaf_count: 0,
                    market_address: Pubkey::new_unique(),
                },
            };

            let mut model: BTreeMap<u128, (Node, Pubkey)> = BTreeMap::new();

            let mut all_keys = vec![];

            let mut rng = StdRng::seed_from_u64(trial);

            assert_eq!(slab.find_min(), None);
            assert_eq!(slab.find_max(), None);

            for i in 0..100 {
                let key = rng.gen();
                let owner = Pubkey::new_unique();
                let qty = rng.gen();
                let callback_info_offset = slab.write_callback_info(&owner.to_bytes()).unwrap();
                let leaf = Node::Leaf(LeafNode {
                    key,
                    callback_info_pt: callback_info_offset,
                    base_quantity: qty,
                });

                println!("key : {:x}", key);
                // println!("owner : {:?}", &owner.to_bytes());
                println!("{}", i);
                slab.insert_leaf(&leaf).unwrap();
                model.insert(key, (leaf, owner)).ok_or(()).unwrap_err();
                all_keys.push(key);

                // test find_by_key
                let valid_search_key = *all_keys.choose(&mut rng).unwrap();
                let invalid_search_key = rng.gen();

                for &search_key in &[valid_search_key, invalid_search_key] {
                    let slab_value = slab
                        .find_by_key(search_key)
                        .and_then(|x| slab.get_node(x))
                        .map(|s| {
                            (
                                s.to_owned(),
                                Pubkey::new(&slab.get_callback_info(
                                    s.as_leaf().unwrap().callback_info_pt as usize,
                                )),
                            )
                        });
                    let model_value = model.get(&search_key).cloned();
                    assert_eq!(slab_value, model_value);
                }

                // test find_min
                let slab_min = slab.get_node(slab.find_min().unwrap()).unwrap().to_owned();
                let model_min = model.iter().next().unwrap().1;
                let owner = Pubkey::new(
                    &slab.get_callback_info(slab_min.as_leaf().unwrap().callback_info_pt as usize),
                );
                assert_eq!(&(slab_min, owner), model_min);

                // test find_max
                let slab_max = slab.get_node(slab.find_max().unwrap()).unwrap().to_owned();
                let model_max = model.iter().next_back().unwrap().1;
                let owner = Pubkey::new(
                    &slab.get_callback_info(slab_max.as_leaf().unwrap().callback_info_pt as usize),
                );
                assert_eq!(&(slab_max, owner), model_max);
            }
        }
    }

    #[test]
    fn simulate_operations() {
        use rand::distributions::WeightedIndex;
        use std::collections::BTreeMap;

        let mut bytes = vec![0u8; 800_000];
        let order_capacity = (bytes.len() - PADDED_SLAB_HEADER_LEN) / (SLOT_SIZE * 2 + 32);

        let callback_memory_offset = PADDED_SLAB_HEADER_LEN + 2 * order_capacity * SLOT_SIZE;
        let slab_data = Rc::new(RefCell::new(&mut bytes[..]));
        let mut slab = Slab {
            buffer: Rc::clone(&slab_data),
            callback_info_len: 32,
            header: SlabHeader {
                account_tag: AccountTag::Asks,
                bump_index: 0,
                free_list_len: 0,
                free_list_head: 0,
                callback_memory_offset: callback_memory_offset as u64,
                callback_free_list_len: 0,
                callback_free_list_head: 0,
                callback_bump_index: callback_memory_offset as u64,
                root_node: 0,
                leaf_count: 0,
                market_address: Pubkey::new_unique(),
            },
        };
        let mut model: BTreeMap<u128, (Node, Pubkey)> = BTreeMap::new();

        let mut all_keys = vec![];
        let mut rng = StdRng::seed_from_u64(1);

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
                let slab_state: Vec<(Node, Pubkey)> = slab.traverse();
                assert_eq!(model_state, slab_state.iter().collect::<Vec<_>>());

                match weights[dist.sample(&mut rng)].0 {
                    op @ Op::InsertNew | op @ Op::InsertDup => {
                        let key = match op {
                            Op::InsertNew => rng.gen(),
                            Op::InsertDup => *all_keys.choose(&mut rng).unwrap(),
                            _ => unreachable!(),
                        };
                        let owner = Pubkey::new_unique();
                        let qty = rng.gen();
                        let callback_info_offset =
                            slab.write_callback_info(&owner.to_bytes()).unwrap();
                        let leaf = Node::Leaf(LeafNode {
                            key,
                            callback_info_pt: callback_info_offset,
                            base_quantity: qty,
                        });

                        println!("Insert {:x}", key);

                        all_keys.push(key);
                        let slab_value = slab
                            .insert_leaf(&leaf)
                            .map(|(_, n)| {
                                n.map(|node| {
                                    let owner = Pubkey::new(&slab.get_callback_info(
                                        node.as_leaf().unwrap().callback_info_pt as usize,
                                    ));
                                    (node, owner)
                                })
                            })
                            .unwrap();
                        let model_value = model.insert(key, (leaf, owner));
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

                        let slab_value = slab.remove_by_key(key).map(|v| v.0);
                        let model_value = model.remove(&key).map(|(n, _)| n);
                        assert_eq!(slab_value, model_value);
                    }
                    Op::Min => {
                        if model.is_empty() {
                            assert_eq!(identity(slab.header.leaf_count), 0);
                        } else {
                            let slab_min =
                                slab.get_node(slab.find_min().unwrap()).unwrap().to_owned();
                            let owner = Pubkey::new(&slab.get_callback_info(
                                slab_min.as_leaf().unwrap().callback_info_pt as usize,
                            ));
                            let model_min = model.iter().next().unwrap().1;
                            assert_eq!(&(slab_min, owner), model_min);
                        }
                    }
                    Op::Max => {
                        if model.is_empty() {
                            assert_eq!(identity(slab.header.leaf_count), 0);
                        } else {
                            let slab_max =
                                slab.get_node(slab.find_max().unwrap()).unwrap().to_owned();
                            let owner = Pubkey::new(&slab.get_callback_info(
                                slab_max.as_leaf().unwrap().callback_info_pt as usize,
                            ));
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
