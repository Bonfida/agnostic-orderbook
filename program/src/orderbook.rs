use crate::{
    critbit::{NodeHandle, Slab},
    state::{MarketState, Side},
};

pub struct OrderBookState<'a> {
    // first byte of a key is 0xaa or 0xbb, disambiguating bids and asks
    pub bids: Slab<'a>,
    pub asks: Slab<'a>,
    pub market_state: MarketState,
}

impl<'ob> OrderBookState<'ob> {
    pub(crate) fn orders_mut(&mut self, side: Side) -> &Slab<'ob> {
        match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        }
    }

    pub(crate) fn find_bbo(&self, side: Side) -> Option<NodeHandle> {
        match side {
            Side::Bid => self.bids.find_max(),
            Side::Ask => self.asks.find_min(),
        }
    }
}
