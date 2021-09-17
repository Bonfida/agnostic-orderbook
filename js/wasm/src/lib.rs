use critbit::Slab;
use std::{cell::RefCell, rc::Rc};

mod critbit;

use wasm_bindgen::prelude::*;

#[cfg(test)]
mod tests {
    #[test]
    #[allow(clippy::eq_op)]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

#[wasm_bindgen]
pub fn find_max(data: &mut [u8], callback_info_len: u64, slot_size: u64) -> Option<u32> {
    let slab = Slab::new(
        Rc::new(RefCell::new(data)),
        callback_info_len as usize,
        slot_size as usize,
    );
    slab.find_max()
}

#[wasm_bindgen]
pub fn find_min(data: &mut [u8], callback_info_len: u64, slot_size: u64) -> Option<u32> {
    let slab = Slab::new(
        Rc::new(RefCell::new(data)),
        callback_info_len as usize,
        slot_size as usize,
    );
    slab.find_min()
}

#[wasm_bindgen]
pub fn find_l2_depth(
    data: &mut [u8],
    callback_info_len: u64,
    slot_size: u64,
    depth: u64,
    increasing: bool,
) -> Vec<u64> {
    let slab = Slab::new(
        Rc::new(RefCell::new(data)),
        callback_info_len as usize,
        slot_size as usize,
    );
    slab.find_l2_depth(depth as usize, increasing)
}
