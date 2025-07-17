use std::collections::binary_heap::BinaryHeap;
use std::cmp::{Ordering, Reverse};
use std::collections::HashMap;
use std::time::SystemTime;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static ORDER_COUNTER: AtomicU64 = AtomicU64::new(1);

use strum_macros::{EnumString, Display};
use parking_lot::RwLock;

#[derive(Clone, Eq, PartialEq, Debug, Hash, Display, EnumString)]
#[strum(serialize_all = "UPPERCASE", ascii_case_insensitive)]
enum Asset {
    AAA,
    BBB,
    CCC,
    DDD,
    EEE,
    FFF,
    GGG
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum Side {
    Buy,
    Sell
}

#[derive(Clone, Debug)]
pub(crate) struct Order {
    pub timestamp: SystemTime,
    pub id: u64,
    pub price: f64,
    pub quantity: u32,
    pub side: Side,
    pub ticker: Asset,
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Order {}

impl Ord for Order {
    fn cmp(&self, other: &Self) -> Ordering {
        self.price.partial_cmp(&other.price).unwrap()
    }
}

impl PartialOrd for Order {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Order {
    pub(crate) fn new(side: Side, price: f64, quantity: u32, ticker: &String) -> Self {
        let id = ORDER_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        Self {
            timestamp: SystemTime::now(),
            id,
            price,
            quantity,
            side,
            ticker: Asset::from_str(ticker).unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
struct OrderBook {
    pub bids: BinaryHeap<Order>,
    pub asks: BinaryHeap<Reverse<Order>>
}

impl OrderBook {
    pub fn match_order(&mut self, mut order: Order) {
        match order.side {
            Side::Buy => {
                while let Some(Reverse(best_ask)) = self.asks.peek() {
                    if order.price >= best_ask.price {
                        let mut best_ask = self.asks.pop().unwrap().0;
                        if best_ask.quantity > order.quantity {
                            best_ask.quantity -= order.quantity;
                            self.asks.push(Reverse(best_ask));
                            return;
                        } else if best_ask.quantity < order.quantity {
                            let remaining = order.quantity - best_ask.quantity;
                            let mut new_order = order.clone();
                            new_order.quantity = remaining;
                            order = new_order;
                            continue;
                        } else {
                            // Fully matched
                            return;
                        }
                    } else {
                        break;
                    }
                }
                self.bids.push(order);
            }
            Side::Sell => {
                while let Some(best_bid) = self.bids.peek() {
                    if order.price <= best_bid.price {
                        let mut best_bid = self.bids.pop().unwrap();
                        if best_bid.quantity > order.quantity {
                            best_bid.quantity -= order.quantity;
                            self.bids.push(best_bid);
                            return;
                        } else if best_bid.quantity < order.quantity {
                            let remaining = order.quantity - best_bid.quantity;
                            let mut new_order = order.clone();
                            new_order.quantity = remaining;
                            order = new_order;
                            continue;
                        } else {
                            // Fully matched
                            return;
                        }
                    } else {
                        break;
                    }
                }
                self.asks.push(Reverse(order));
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Exchange {
    pub books: HashMap<Asset, Arc<RwLock<OrderBook>>>,
}

impl Exchange {
    pub fn submit_order(&mut self, order: Order) {
        let book = self.books.entry(order.ticker.clone()).or_insert_with(|| {
            Arc::new(RwLock::new(OrderBook {
                bids: BinaryHeap::new(),
                asks: BinaryHeap::new(),
            }))
        });
        book.write().match_order(order);
    }

    pub fn cancel_order(&mut self, ticker: Asset, side: Side, order_id: u64) -> bool {
        if let Some(book_lock) = self.books.get(&ticker) {
            let mut book = book_lock.write();
            match side {
                Side::Buy => {
                    let mut new_bids = BinaryHeap::new();
                    let mut removed = false;
                    while let Some(order) = book.bids.pop() {
                        if order.id == order_id {
                            removed = true;
                            continue;
                        }
                        new_bids.push(order);
                    }
                    book.bids = new_bids;
                    return removed;
                }
                Side::Sell => {
                    let mut new_asks = BinaryHeap::new();
                    let mut removed = false;
                    while let Some(Reverse(order)) = book.asks.pop() {
                        if order.id == order_id {
                            removed = true;
                            continue;
                        }
                        new_asks.push(Reverse(order));
                    }
                    book.asks = new_asks;
                    return removed;
                }
            }
        }
        false
    }

    pub fn get_order_book(&self, ticker: &Asset) -> Option<Arc<RwLock<OrderBook>>> {
        self.books.get(ticker).cloned()
    }

    pub fn get_bids(&self, ticker: &Asset) -> Option<Vec<Order>> {
        self.books.get(ticker).map(|book| {
            let book = book.read();
            book.bids.iter().cloned().collect()
        })
    }

    pub fn get_asks(&self, ticker: &Asset) -> Option<Vec<Order>> {
        self.books.get(ticker).map(|book| {
            let book = book.read();
            book.asks.iter().map(|r| r.0.clone()).collect()
        })
    }
}