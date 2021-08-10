use crate::error::AOError;
use borsh::{BorshDeserialize, BorshSerialize};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use std::convert::TryFrom;
use std::{cell::RefCell, convert::identity, mem::size_of, num::NonZeroU64, rc::Rc};
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

#[derive(BorshDeserialize, BorshSerialize)]
pub struct LeafNode {
    key: u128,
    owner: Pubkey,
    quantity: u64,
}

pub const SLOT_SIZE: usize = size_of::<LeafNode>() + 4; // Account for the tag

impl LeafNode {
    pub fn new(key: u128, owner: Pubkey, quantity: u64) -> Self {
        LeafNode {
            key,
            owner,
            quantity,
        }
    }

    pub fn price(&self) -> NonZeroU64 {
        NonZeroU64::new((self.key >> 64) as u64).unwrap()
    }

    pub fn order_id(&self) -> u128 {
        self.key
    }

    pub fn quantity(&self) -> u64 {
        self.quantity
    }

    pub fn set_quantity(&mut self, quantity: u64) {
        self.quantity = quantity;
    }

    pub fn owner(&self) -> Pubkey {
        self.owner
    }

    pub fn to_any(&self) -> AnyNode {
        //TODO retire
        let mut data = Vec::new();
        self.serialize(&mut data).unwrap();
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
    fn key(&self) -> Option<u128> {
        match self.case()? {
            Node::Inner(inner) => Some(inner.key),
            Node::Leaf(leaf) => Some(leaf.key),
        }
    }

    #[cfg(test)]
    fn prefix_len(&self) -> u32 {
        match self.case().unwrap() {
            Node::Inner(InnerNode { prefix_len, .. }) => prefix_len,
            Node::Leaf(_) => 128,
        }
    }

    fn children(&self) -> Option<[u32; 2]> {
        match self.case().unwrap() {
            Node::Inner(InnerNode { children, .. }) => Some(children),
            Node::Leaf(_) => None,
        }
    }

    fn case(&'a self) -> Option<Node> {
        match NodeTag::try_from(self.tag) {
            Ok(NodeTag::InnerNode) => {
                let inner_node = InnerNode::deserialize(&mut &self.data[..]).unwrap();
                Some(Node::Inner(inner_node))
            }
            Ok(NodeTag::LeafNode) => {
                let leaf_node = LeafNode::deserialize(&mut &self.data[..]).unwrap();
                Some(Node::Leaf(leaf_node))
            }
            _ => None,
        }
    }

    fn case_mut(&mut self) -> Option<Node> {
        match NodeTag::try_from(self.tag) {
            Ok(NodeTag::InnerNode) => {
                let inner_node = InnerNode::deserialize(&mut &self.data[..]).unwrap();
                Some(Node::Inner(inner_node))
            }
            Ok(NodeTag::LeafNode) => {
                let leaf_node = LeafNode::deserialize(&mut &self.data[..]).unwrap();
                Some(Node::Leaf(leaf_node))
            }
            _ => None,
        }
    }

    pub fn as_leaf(&self) -> Option<LeafNode> {
        match self.case() {
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

pub struct Slab<'a>(Rc<RefCell<&'a mut [u8]>>);

// Data access methods
impl<'a> Slab<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        let len_without_header = bytes.len().checked_sub(SLAB_HEADER_LEN).unwrap();
        let slop = len_without_header % size_of::<AnyNode>();
        let truncated_len = bytes.len() - slop;
        let bytes = &mut bytes[..truncated_len];
        Slab(Rc::new(RefCell::new(bytes)))
    }

    pub fn new_from_acc_info(acc_info: AccountInfo<'a>) -> Self {
        let len_without_header = acc_info.data_len().checked_sub(SLAB_HEADER_LEN).unwrap();
        assert_eq!(len_without_header % size_of::<AnyNode>(), 0); // TODO either truncate or throw
        Slab(acc_info.data)
    }

    fn get_header(&self) -> SlabHeader {
        SlabHeader::deserialize(&mut &self.0.borrow()[..SLAB_HEADER_LEN]).unwrap()
    }
}

// Tree nodes manipulation methods
impl<'a> Slab<'a> {
    fn capacity(&self) -> u64 {
        ((self.0.borrow().len() - SLAB_HEADER_LEN) % SLOT_SIZE) as u64
    }

    fn clear(&mut self) {
        let header = &mut self.get_header();
        *header = SlabHeader {
            bump_index: 0,
            free_list_len: 0,
            free_list_head: 0,

            root_node: 0,
            leaf_count: 0,
            market_address: Pubkey::new_from_array([0; 32]),
        }
    }

    fn is_empty(&self) -> bool {
        let SlabHeader {
            bump_index,
            free_list_len,
            ..
        } = self.get_header();
        bump_index == free_list_len
    }

    fn get_node(&self, key: u32) -> Option<AnyNode> {
        let offset = SLAB_HEADER_LEN + key as usize;
        let node = AnyNode::deserialize(&mut &self.0.borrow()[offset..offset + SLOT_SIZE]).unwrap();
        let tag = NodeTag::try_from(node.tag);
        match tag {
            Ok(NodeTag::InnerNode) | Ok(NodeTag::LeafNode) => Some(node),
            _ => None,
        }
    }

    fn write_node(&mut self, node: &AnyNode, key: u32) {
        let offset = SLAB_HEADER_LEN + key as usize;
        self.0.borrow_mut()[offset..offset + 4].copy_from_slice(&node.tag.to_le_bytes());
        self.0.borrow_mut()[offset + 4..offset + SLOT_SIZE].copy_from_slice(&node.data);
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
        let free_node_ref = &mut FreeNode::deserialize(&mut &any_node_ref.data[..]).unwrap();
        any_node_ref.tag = if header.free_list_len == 0 {
            NodeTag::LastFreeNode.into()
        } else {
            NodeTag::FreeNode.into()
        };
        free_node_ref.next = header.free_list_head;
        header.free_list_len += 1;
        header.free_list_head = key;
        Some(val)
    }

    fn contains(&self, key: u32) -> bool {
        self.get_node(key).is_some()
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
            match root_contents.case().unwrap() {
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
            let root_key = root_contents.key().unwrap();
            if root_key == new_leaf.key {
                if let Some(Node::Leaf(old_root_as_leaf)) = root_contents.case() {
                    // clobber the existing leaf
                    self.write_node(&new_leaf.to_any(), root);
                    return Ok((root, Some(old_root_as_leaf)));
                }
            }
            let shared_prefix_len: u32 = (root_key ^ new_leaf.key).leading_zeros();
            match root_contents.case() {
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

    #[cfg(test)]
    fn find_by_key(&self, search_key: u128) -> Option<NodeHandle> {
        let mut node_handle: NodeHandle = self.root()?;
        loop {
            let node_ref = self.get_node(node_handle).unwrap();
            let node_prefix_len = node_ref.prefix_len();
            let node_key = node_ref.key().unwrap();
            let common_prefix_len = (search_key ^ node_key).leading_zeros();
            if common_prefix_len < node_prefix_len {
                return None;
            }
            match node_ref.case().unwrap() {
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

    pub(crate) fn find_by<F: Fn(&LeafNode) -> bool>(
        &self,
        limit: &mut u16,
        predicate: F,
    ) -> Vec<u128> {
        let mut found = Vec::new();
        let mut nodes_to_search: Vec<NodeHandle> = Vec::new();
        let mut current_node: Option<AnyNode>;

        let top_node = self.root();

        // No found nodes.
        if top_node.is_none() {
            return found;
        }

        nodes_to_search.push(top_node.unwrap());

        // Search through the tree.
        while !nodes_to_search.is_empty() && *limit > 0 {
            *limit -= 1;

            current_node = self.get_node(nodes_to_search.pop().unwrap());

            // Node not found.
            if current_node.is_none() {
                break;
            }

            match current_node.unwrap().case().unwrap() {
                Node::Leaf(leaf) if predicate(&leaf) => {
                    // Found a matching leaf.
                    found.push(leaf.key)
                }
                Node::Inner(inner) => {
                    // Search the children.
                    nodes_to_search.push(inner.children[0]);
                    nodes_to_search.push(inner.children[1]);
                }
                _ => (),
            }
        }

        found
    }

    pub fn remove_by_key(&mut self, search_key: u128) -> Option<LeafNode> {
        let mut parent_h = self.root()?;
        let mut child_h;
        let mut crit_bit;
        match self.get_node(parent_h).unwrap().case().unwrap() {
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
            match self.get_node(child_h).unwrap().case().unwrap() {
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
        let other_child_h =
            self.get_node(parent_h).unwrap().children().unwrap()[!crit_bit as usize];
        let other_child_node_contents = self.remove(other_child_h).unwrap();
        self.write_node(&other_child_node_contents, parent_h);
        self.get_header().leaf_count -= 1;
        let removed_leaf = LeafNode::deserialize(&mut &self.remove(child_h).unwrap().data[..]);
        Some(removed_leaf.unwrap())
    }

    pub fn remove_min(&mut self) -> Option<LeafNode> {
        self.remove_by_key(self.get_node(self.find_min()?)?.key()?)
    }

    pub fn remove_max(&mut self) -> Option<LeafNode> {
        self.remove_by_key(self.get_node(self.find_max()?)?.key()?)
    }

    #[cfg(test)]
    fn traverse(&self) -> Vec<LeafNode> {
        fn walk_rec<'a>(slab: &'a Slab, sub_root: NodeHandle, buf: &mut Vec<LeafNode>) {
            match slab.get_node(sub_root).unwrap().case().unwrap() {
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
        self.get_header().serialize(&mut header_data);
        hexdump::hexdump(&header_data);
        println!("Data:");
        hexdump::hexdump(&self.0.borrow());
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
            assert!(node.prefix_len() > last_prefix_len);
            let node_key = node.key().unwrap();
            assert_eq!(
                last_crit_bit,
                (node_key & ((1u128 << 127) >> last_prefix_len)) != 0
            );
            let prefix_mask = (((((1u128) << 127) as i128) >> last_prefix_len) as u128) << 1;
            assert_eq!(last_prefix & prefix_mask, node.key().unwrap() & prefix_mask);
            if let Some([c0, c1]) = node.children() {
                check_rec(slab, c0, node.prefix_len(), node_key, false, count);
                check_rec(slab, c1, node.prefix_len(), node_key, true, count);
            }
        }
        if let Some(root) = self.root() {
            count += 1;
            let node = self.get_node(root).unwrap();
            let node_key = node.key().unwrap();
            if let Some([c0, c1]) = node.children() {
                check_rec(self, c0, node.prefix_len(), node_key, false, &mut count);
                check_rec(self, c1, node.prefix_len(), node_key, true, &mut count);
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use bytemuck::bytes_of;
//     use rand::prelude::*;

//     #[test]
//     fn simulate_find_min() {
//         use std::collections::BTreeMap;

//         for trial in 0..10u64 {
//             let mut aligned_buf = vec![0u64; 10_000];
//             let bytes: &mut [u8] = cast_slice_mut(aligned_buf.as_mut_slice());

//             let slab: &mut Slab = Slab::new(bytes);
//             let mut model: BTreeMap<u128, LeafNode> = BTreeMap::new();

//             let mut all_keys = vec![];

//             let mut rng = StdRng::seed_from_u64(trial);

//             assert_eq!(slab.find_min(), None);
//             assert_eq!(slab.find_max(), None);

//             for i in 0..100 {
//                 let offset = rng.gen();
//                 let key = rng.gen();
//                 let owner = rng.gen();
//                 let qty = rng.gen();
//                 let leaf = LeafNode::new(offset, key, owner, qty);

//                 println!("{:x}", key);
//                 println!("{}", i);

//                 slab.insert_leaf(&leaf).unwrap();
//                 model.insert(key, leaf).ok_or(()).unwrap_err();
//                 all_keys.push(key);

//                 // test find_by_key
//                 let valid_search_key = *all_keys.choose(&mut rng).unwrap();
//                 let invalid_search_key = rng.gen();

//                 for &search_key in &[valid_search_key, invalid_search_key] {
//                     let slab_value = slab
//                         .find_by_key(search_key)
//                         .map(|x| slab.get_node(x))
//                         .flatten()
//                         .map(bytes_of);
//                     let model_value = model.get(&search_key).map(bytes_of);
//                     assert_eq!(slab_value, model_value);
//                 }

//                 // test find_min
//                 let slab_min = slab.get_node(slab.find_min().unwrap()).unwrap();
//                 let model_min = model.iter().next().unwrap().1;
//                 assert_eq!(bytes_of(slab_min), bytes_of(model_min));

//                 // test find_max
//                 let slab_max = slab.get_node(slab.find_max().unwrap()).unwrap();
//                 let model_max = model.iter().next_back().unwrap().1;
//                 assert_eq!(bytes_of(slab_max), bytes_of(model_max));
//             }
//         }
//     }

//     #[test]
//     fn simulate_operations() {
//         use rand::distributions::WeightedIndex;
//         use std::collections::BTreeMap;

//         let mut aligned_buf = vec![0u64; 1_250_000];
//         let bytes: &mut [u8] = &mut cast_slice_mut(aligned_buf.as_mut_slice());
//         let slab: &mut Slab = Slab::new(bytes);
//         let mut model: BTreeMap<u128, LeafNode> = BTreeMap::new();

//         let mut all_keys = vec![];
//         let mut rng = StdRng::seed_from_u64(0);

//         #[derive(Copy, Clone)]
//         enum Op {
//             InsertNew,
//             InsertDup,
//             Delete,
//             Min,
//             Max,
//             End,
//         }

//         for weights in &[
//             [
//                 (Op::InsertNew, 2000),
//                 (Op::InsertDup, 200),
//                 (Op::Delete, 2210),
//                 (Op::Min, 500),
//                 (Op::Max, 500),
//                 (Op::End, 1),
//             ],
//             [
//                 (Op::InsertNew, 10),
//                 (Op::InsertDup, 200),
//                 (Op::Delete, 5210),
//                 (Op::Min, 500),
//                 (Op::Max, 500),
//                 (Op::End, 1),
//             ],
//         ] {
//             let dist = WeightedIndex::new(weights.iter().map(|(_op, wt)| wt)).unwrap();

//             for i in 0..100_000 {
//                 slab.check_invariants();
//                 let model_state = model.values().collect::<Vec<_>>();
//                 let slab_state = slab.traverse();
//                 assert_eq!(model_state, slab_state);

//                 match weights[dist.sample(&mut rng)].0 {
//                     op @ Op::InsertNew | op @ Op::InsertDup => {
//                         let offset = rng.gen();
//                         let key = match op {
//                             Op::InsertNew => rng.gen(),
//                             Op::InsertDup => *all_keys.choose(&mut rng).unwrap(),
//                             _ => unreachable!(),
//                         };
//                         let owner = rng.gen();
//                         let qty = rng.gen();
//                         let leaf = LeafNode::new(offset, key, owner, qty);

//                         println!("Insert {:x}", key);

//                         all_keys.push(key);
//                         let slab_value = slab.insert_leaf(&leaf).unwrap().1;
//                         let model_value = model.insert(key, leaf);
//                         if slab_value != model_value {
//                             slab.hexdump();
//                         }
//                         assert_eq!(slab_value, model_value);
//                     }
//                     Op::Delete => {
//                         let key = all_keys
//                             .choose(&mut rng)
//                             .map(|x| *x)
//                             .unwrap_or_else(|| rng.gen());

//                         println!("Remove {:x}", key);

//                         let slab_value = slab.remove_by_key(key);
//                         let model_value = model.remove(&key);
//                         assert_eq!(slab_value.as_ref().map(cast_ref), model_value.as_ref());
//                     }
//                     Op::Min => {
//                         if model.len() == 0 {
//                             assert_eq!(identity(slab.get_header().leaf_count), 0);
//                         } else {
//                             let slab_min = slab.get_node(slab.find_min().unwrap()).unwrap();
//                             let model_min = model.iter().next().unwrap().1;
//                             assert_eq!(bytes_of(slab_min), bytes_of(model_min));
//                         }
//                     }
//                     Op::Max => {
//                         if model.len() == 0 {
//                             assert_eq!(identity(slab.get_header().leaf_count), 0);
//                         } else {
//                             let slab_max = slab.get_node(slab.find_max().unwrap()).unwrap();
//                             let model_max = model.iter().next_back().unwrap().1;
//                             assert_eq!(bytes_of(slab_max), bytes_of(model_max));
//                         }
//                     }
//                     Op::End => {
//                         if i > 10_000 {
//                             break;
//                         }
//                     }
//                 }
//             }
//         }
//     }
// }
