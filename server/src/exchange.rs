use std::collections::{BTreeMap, VecDeque, HashMap};
use std::cmp::{Ordering, PartialEq};

use ordered_float::OrderedFloat;
use serde::{Serialize, Deserialize};
use fefix::definitions::fix50::*;
use fefix::fix_values::Timestamp;

use crate::engine::EngineMessage;
use crate::types::*;

#[derive(Clone, Debug)]
pub(crate) struct Order {
    order_id: OrderID,
    price: Price,
    quantity: Quantity,
    send_timestamp: Timestamp,
    receive_timestamp: Timestamp,
    side: Side,
    order_type: OrdType,
    time_in_force: TimeInForce,
    exec_instruction: ExecInst,
    instrument_id: InstrumentID,
    account_id: AccountID,
    sender_id: ClientID,
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        self.order_id == other.order_id
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
    bids: BTreeMap<OrderedFloat<f64>, VecDeque<Order>>, // descending order if needed
    asks: BTreeMap<OrderedFloat<f64>, VecDeque<Order>>, // ascending order
    order_index: HashMap<u64, (Side, f64)>,
}

impl PartialEq<ClientID> for AccountID {
    fn eq(&self, other: &ClientID) -> bool {
        todo!()
    }
}

impl OrderBook {
    fn match_order(&mut self, mut order: Order) {
        // Handle Stop orders
        if let OrdType::Stop = order.order_type {
            match order.side {
                Side::Buy => {
                    if let Some((&best_ask_price, _)) = self.asks.iter().next() {
                        if best_ask_price < OrderedFloat(order.price) {
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
                    if let Some((&best_bid_price, _)) = self.bids.iter().next_back() {
                        if best_bid_price > OrderedFloat(order.price) {
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
                    if let Some((&best_ask_price, _)) = self.asks.iter().next() {
                        if best_ask_price < OrderedFloat(order.price) {
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
                    if let Some((&best_bid_price, _)) = self.bids.iter().next_back() {
                        if best_bid_price > OrderedFloat(order.price) {
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
                while order.quantity > 0 {
                    let best_ask_price = if order.order_type == OrdType::Market {
                        self.asks.keys().next().cloned()
                    } else {
                        self.asks.keys().next().filter(|&p| OrderedFloat(order.price) >= *p).cloned()
                    };
                    if let Some(price) = best_ask_price {
                        let queue = self.asks.get_mut(&price).unwrap();
                        while order.quantity > 0 && !queue.is_empty() {
                            if let Some(mut best_ask) = queue.pop_front() {
                                if best_ask.quantity > order.quantity {
                                    best_ask.quantity -= order.quantity;
                                    order.quantity = 0;
                                    queue.push_front(best_ask);
                                } else if best_ask.quantity < order.quantity {
                                    order.quantity -= best_ask.quantity;
                                    self.order_index.remove(&best_ask.order_id);
                                } else {
                                    order.quantity = 0;
                                    self.order_index.remove(&best_ask.order_id);
                                }
                            }
                        }
                        if queue.is_empty() {
                            self.asks.remove(&price);
                        }
                        if order.quantity == 0 {
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
                            self.bids.entry(OrderedFloat(order.price)).or_default().push_back(order.clone());
                            self.order_index.insert(order.order_id, (Side::Buy, order.price));
                        }
                    }
                }
            }
            Side::Sell => {
                while order.quantity > 0 {
                    let best_bid_price = if order.order_type == OrdType::Market {
                        self.bids.keys().next_back().cloned()
                    } else {
                        self.bids.keys().next_back().filter(|&p| OrderedFloat(order.price) <= *p).cloned()
                    };
                    if let Some(price) = best_bid_price {
                        let queue = self.bids.get_mut(&price).unwrap();
                        while order.quantity > 0 && !queue.is_empty() {
                            if let Some(mut best_bid) = queue.pop_front() {
                                if best_bid.quantity > order.quantity {
                                    best_bid.quantity -= order.quantity;
                                    order.quantity = 0;
                                    queue.push_front(best_bid);
                                } else if best_bid.quantity < order.quantity {
                                    order.quantity -= best_bid.quantity;
                                    self.order_index.remove(&best_bid.order_id);
                                } else {
                                    order.quantity = 0;
                                    self.order_index.remove(&best_bid.order_id);
                                }
                            }
                        }
                        if queue.is_empty() {
                            self.bids.remove(&price);
                        }
                        if order.quantity == 0 {
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
                            self.asks.entry(OrderedFloat(order.price)).or_default().push_back(order.clone());
                            self.order_index.insert(order.order_id, (Side::Sell, order.price));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn remove_order(&mut self, order_id: u64, client_id: ClientID) -> bool {
        if let Some((side, price)) = self.order_index.get(&order_id).cloned() {
            let queue_opt = match side {
                Side::Buy => self.bids.get_mut(&OrderedFloat(price)),
                Side::Sell => self.asks.get_mut(&OrderedFloat(price)),
                _ => None,
            };
            if let Some(queue) = queue_opt {
                let pos = queue.iter().position(|o| o.order_id == order_id && o.account_id == client_id);
                if let Some(idx) = pos {
                    queue.remove(idx);
                    if queue.is_empty() {
                        match side {
                            Side::Buy => { self.bids.remove(&OrderedFloat(price)); }
                            Side::Sell => { self.asks.remove(&OrderedFloat(price)); }
                            _ => {}
                        }
                    }
                    self.order_index.remove(&order_id);
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Bankroll {
    pub cash: f64,
    pub positions: HashMap<String, i64>, // instrument -> quantity
}

#[derive(Debug)]
pub struct Exchange {
    order_counter: u64,
    books: HashMap<String, OrderBook>,
}

impl Exchange {
    pub fn new() -> Self {
        Self {
            order_counter: 1,
            books: HashMap::new(),
        }
    }

    pub fn handle_message(&mut self, message: EngineMessage) -> Option<EngineMessage> {
        match message {
            EngineMessage::CreateInstrument { instrument_id, .. } => {
                // Extract sending_time and receiving_time if present (future logic)
                self.books.entry(instrument_id).or_insert_with(|| OrderBook {
                    bids: BTreeMap::new(),
                    asks: BTreeMap::new(),
                    order_index: HashMap::new(),
                });
                None
            }
            EngineMessage::NewOrder {
                sending_time,
                receiving_time,
                client_id,
                account_id,
                instrument_id,
                order_type,
                side,
                quantity,
                price,
                time_in_force,
            } => {
                // Extract sending_time and receiving_time at the beginning of the branch
                let receiving_time = receiving_time;

                if !self.books.contains_key(&instrument_id) {
                    return Some(EngineMessage::OrderRejected {
                        reason: "Unknown instrument".to_string(),
                        client_id,
                    });
                }

                let order_id = self.order_counter;
                self.order_counter += 1;

                let order = Order {
                    order_id: order_id.clone(),
                    send_timestamp: sending_time,
                    receive_timestamp: receiving_time,
                    price: price.unwrap_or(0.0),
                    quantity,
                    side,
                    order_type,
                    time_in_force: time_in_force.unwrap_or(TimeInForce::Day),
                    exec_instruction: ExecInst::StayOnOfferSide,
                    instrument_id: instrument_id.clone(),
                    account_id: account_id,
                    sender_id: client_id.clone(),
                };

                let book = self.books.get_mut(&instrument_id).unwrap();
                book.match_order(order);
                Some(EngineMessage::OrderAccepted {
                    client_id,
                    order_id
                })
            }
            EngineMessage::CancelOrder {
                sending_time,
                receiving_time,
                order_id,
                client_id: client_id,
                ..
            } => {
                // Extract sending_time and receiving_time at the beginning of the branch (future logic)
                let _sending_time = sending_time;
                let _receiving_time = receiving_time;
                for (_instrument, book) in &mut self.books {
                    let removed = book.remove_order(order_id, client_id.to_owned());
                    if removed {
                        return Some(EngineMessage::OrderCancelled {
                            order_id,
                            client_id: client_id.clone(),
                        });
                    }
                }
                Some(EngineMessage::OrderRejected {
                    reason: "Order not found".to_string(),
                    client_id: client_id.clone(),
                })
            }
            EngineMessage::AmendOrder {
                sending_time,
                receiving_time,
                order_id,
                new_quantity,
                new_price,
                time_in_force,
                ..
            } => {
                // Extract sending_time and receiving_time at the beginning of the branch (future logic)
                let _sending_time = sending_time;
                let _receiving_time = receiving_time;
                // Amend logic not implemented yet
                Some(EngineMessage::LogEvent {
                    message: "Amend not yet implemented".to_string(),
                })
            }
            EngineMessage::AdvanceTime { sending_time, receiving_time, timestamp, .. } => {
                // Extract sending_time and receiving_time at the beginning of the branch (future logic)
                let _sending_time = sending_time;
                let _receiving_time = receiving_time;
                // AdvanceTime logic not implemented yet
                Some(EngineMessage::LogEvent {
                    message: "AdvanceTime not yet implemented".to_string(),
                })
            }
            EngineMessage::LogEvent { message: log_message, .. } => {
                // Extract sending_time and receiving_time at the beginning of the branch (future logic)
                Some(EngineMessage::LogEvent {
                    message: log_message,
                })
            }
            _ => Some(EngineMessage::LogEvent {
                message: "Unsupported message received".to_string(),
            }),
        }
    }
}