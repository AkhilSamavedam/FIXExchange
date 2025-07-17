use std::collections::binary_heap::BinaryHeap;
use std::cmp::{Ordering, Reverse};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use serde::{Serialize, Deserialize};
use parking_lot::{RwLock};
use fefix::definitions::fix50::*;
use fefix::fix_values::Timestamp;


#[derive(Clone, Debug)]
pub(crate) struct Order {
    server_order_id: u64,
    client_order_id: u64,
    send_timestamp: Timestamp,
    receive_timestamp: Timestamp,
    price: f64,
    quantity: u32,
    side: Side,
    order_type: OrdType,
    time_in_force: TimeInForce,
    exec_instruction: ExecInst,
    instrument_id: String,
    account_id: String,
    sender_org_id: String,
    sender_sub_id: String,
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        self.server_order_id == other.server_order_id
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

#[derive(Clone, Debug)]
struct OrderBook {
    bids: BinaryHeap<Order>,
    asks: BinaryHeap<Reverse<Order>>
}

impl OrderBook {
    fn match_order(&mut self, mut order: Order) {
        // Handle Stop orders
        if let OrdType::Stop = order.order_type {
            match order.side {
                Side::Buy => {
                    if let Some(Reverse(best_ask)) = self.asks.peek() {
                        if best_ask.price < order.price {
                            // Not triggered yet, buffer order
                            return;
                        }
                    } else {
                        // No market price, cannot trigger
                        return;
                    }
                    // Triggered, convert to Market order for matching
                    order.order_type = OrdType::Market;
                }
                Side::Sell => {
                    if let Some(best_bid) = self.bids.peek() {
                        if best_bid.price > order.price {
                            // Not triggered yet, buffer order
                            return;
                        }
                    } else {
                        // No market price, cannot trigger
                        return;
                    }
                    // Triggered, convert to Market order for matching
                    order.order_type = OrdType::Market;
                }
                _ => {}
            }
        }

        // Handle StopLimit orders
        else if let OrdType::StopLimit = order.order_type {
            match order.side {
                Side::Buy => {
                    if let Some(Reverse(best_ask)) = self.asks.peek() {
                        if best_ask.price < order.price {
                            // Not triggered yet, buffer order
                            return;
                        }
                    } else {
                        // No market price, cannot trigger
                        return;
                    }
                    // Triggered, convert to Limit order for matching
                    order.order_type = OrdType::Limit;
                }
                Side::Sell => {
                    if let Some(best_bid) = self.bids.peek() {
                        if best_bid.price > order.price {
                            // Not triggered yet, buffer order
                            return;
                        }
                    } else {
                        // No market price, cannot trigger
                        return;
                    }
                    // Triggered, convert to Limit order for matching
                    order.order_type = OrdType::Limit;
                }
                _ => {}
            }
        }

        // Now proceed to matching logic
        match order.side {
            Side::Buy => {
                let mut total_quantity_matched = 0;

                while let Some(Reverse(best_ask)) = self.asks.peek() {
                    if order.order_type == OrdType::Market
                        || order.price >= best_ask.price
                    {
                        let mut best_ask = self.asks.pop().unwrap().0;

                        if best_ask.quantity > order.quantity {
                            best_ask.quantity -= order.quantity;
                            total_quantity_matched += order.quantity;
                            self.asks.push(Reverse(best_ask));
                            order.quantity = 0;
                            break;
                        } else if best_ask.quantity < order.quantity {
                            total_quantity_matched += best_ask.quantity;
                            let remaining = order.quantity - best_ask.quantity;
                            order.quantity = remaining;
                            continue;
                        } else {
                            // Fully matched
                            total_quantity_matched += best_ask.quantity;
                            order.quantity = 0;
                            break;
                        }
                    } else {
                        break;
                    }
                }

                match order.time_in_force {
                    TimeInForce::ImmediateOrCancel => {
                        // Immediate or Cancel: discard any unfilled quantity
                        if order.quantity > 0 {
                            // Discard remaining quantity
                            return;
                        } else {
                            // Fully or partially matched, no further action needed
                            return;
                        }
                    }
                    TimeInForce::FillOrKill => {
                        // Fill or Kill: if not fully filled, discard entire order
                        if order.quantity > 0 {
                            // Rollback any partial fills by re-adding asks consumed
                            // Since we don't track partial fills separately, for simplicity, discard entire order without adding to book
                            return;
                        } else {
                            // Fully filled
                            return;
                        }
                    }
                    _ => {
                        // Other TIF: post remaining quantity to book
                        if order.quantity > 0 {
                            self.bids.push(order);
                        }
                    }
                }
            }
            Side::Sell => {
                let mut total_quantity_matched = 0;

                while let Some(best_bid) = self.bids.peek() {
                    if order.order_type == OrdType::Market
                        || order.price <= best_bid.price
                    {
                        let mut best_bid = self.bids.pop().unwrap();

                        if best_bid.quantity > order.quantity {
                            best_bid.quantity -= order.quantity;
                            total_quantity_matched += order.quantity;
                            self.bids.push(best_bid);
                            order.quantity = 0;
                            break;
                        } else if best_bid.quantity < order.quantity {
                            total_quantity_matched += best_bid.quantity;
                            let remaining = order.quantity - best_bid.quantity;
                            order.quantity = remaining;
                            continue;
                        } else {
                            // Fully matched
                            total_quantity_matched += best_bid.quantity;
                            order.quantity = 0;
                            break;
                        }
                    } else {
                        break;
                    }
                }

                match order.time_in_force {
                    TimeInForce::ImmediateOrCancel => {
                        // Immediate or Cancel: discard any unfilled quantity
                        if order.quantity > 0 {
                            // Discard remaining quantity
                            return;
                        } else {
                            // Fully or partially matched, no further action needed
                            return;
                        }
                    }
                    TimeInForce::FillOrKill => {
                        // Fill or Kill: if not fully filled, discard entire order
                        if order.quantity > 0 {
                            // Rollback any partial fills by re-adding bids consumed
                            // Since we don't track partial fills separately, for simplicity, discard entire order without adding to book
                            return;
                        } else {
                            // Fully filled
                            return;
                        }
                    }
                    _ => {
                        // Other TIF: post remaining quantity to book
                        if order.quantity > 0 {
                            self.asks.push(Reverse(order));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct Exchange {
    order_counter: AtomicU64,
    books_lock: RwLock<()>,
    books: HashMap<String, Arc<RwLock<OrderBook>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Bankroll {
    pub cash: f64,
    pub positions: HashMap<String, i64>, // instrument -> quantity
}


impl Exchange {
    pub fn new() -> Self {
        Self {
            order_counter: AtomicU64::new(1),
            books_lock: RwLock::new(()),
            books: HashMap::new(),
        }
    }

    pub(crate) fn create_order(
        &mut self,
        client_order_id: u64,
        send_timestamp: Timestamp,
        price: f64,
        quantity: u32,
        side: Side,
        order_type: OrdType,
        time_in_force: TimeInForce,
        exec_instruction: ExecInst,
        instrument_id: String,
        account_id: String,
        sender_org_id: String,
        sender_sub_id: String,
    ) -> Order {

        Order {
            server_order_id: self.order_counter.fetch_add(1, AtomicOrdering::Relaxed),
            client_order_id: client_order_id,
            send_timestamp: send_timestamp,
            receive_timestamp: Timestamp::utc_now(),
            price: price,
            quantity: quantity,
            side: side,
            order_type: order_type,
            time_in_force: time_in_force,
            exec_instruction: exec_instruction,
            instrument_id: instrument_id,
            account_id: account_id,
            sender_org_id: sender_org_id,
            sender_sub_id: sender_sub_id
        }
    }

    pub(crate) fn submit_order(&mut self, order: Order) {
        let _ = self.books_lock.read();
        if !self.books.contains_key(&order.instrument_id) {
            // Instrument does not exist, reject order
            return;
        }

        if order.order_type == OrdType::Market {
            if let Some(book) = self.books.get(&order.instrument_id) {
                let book = book.read();
                match order.side {
                    Side::Buy => {
                        if book.asks.is_empty() {
                            return;
                        }
                    }
                    Side::Sell => {
                        if book.bids.is_empty() {
                            return;
                        }
                    }
                    _ => {}
                }
            } else {
                return;
            }
        }
        let book = self.books.get(&order.instrument_id).unwrap();
        book.write().match_order(order);
    }

    pub(crate) fn cancel_order(&mut self, ticker: String, side: Side, order_id: u64) -> bool {
        let _ = self.books_lock.read();
        if let Some(book_lock) = self.books.get(&ticker) {
            let mut book = book_lock.write();
            match side {
                Side::Buy => {
                    let mut new_bids = BinaryHeap::new();
                    let mut removed = false;
                    while let Some(order) = book.bids.pop() {
                        if order.server_order_id == order_id {
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
                        if order.server_order_id == order_id {
                            removed = true;
                            continue;
                        }
                        new_asks.push(Reverse(order));
                    }
                    book.asks = new_asks;
                    return removed;
                }
                _ => {}
            }
        }
        false
    }

    pub(crate) fn get_order_book(&self, ticker: &str) -> Option<Arc<RwLock<OrderBook>>> {
        let _ = self.books_lock.read();
        self.books.get(ticker).cloned()
    }

    pub(crate) fn get_bids(&self, ticker: &str) -> Option<Vec<Order>> {
        let _ = self.books_lock.read();
        self.books.get(ticker).map(|book| {
            let book = book.read();
            book.bids.iter().cloned().collect()
        })
    }

    pub(crate) fn get_asks(&self, ticker: &str) -> Option<Vec<Order>> {
        let _ = self.books_lock.read();
        self.books.get(ticker).map(|book| {
            let book = book.read();
            book.asks.iter().map(|r| r.0.clone()).collect()
        })
    }

    pub(crate) fn create_instrument(&mut self, symbol: &str) {
        let _ = self.books_lock.write();
        self.books.entry(symbol.to_string()).or_insert_with(|| {
            Arc::new(RwLock::new(OrderBook {
                bids: BinaryHeap::new(),
                asks: BinaryHeap::new(),
            }))
        });
    }
}