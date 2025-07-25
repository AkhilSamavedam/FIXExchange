use std::collections::{BTreeMap, VecDeque, HashMap};
use std::cmp::{Ordering, PartialEq};

use fefix::definitions::fix50::*;
use fefix::fix_values::Timestamp;

use crate::engine::EngineMessage;
use crate::types::*;

#[derive(Clone, Debug)]
struct Order {
    order_id: OrderID,
    client_order_id: ClOrdID,
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
    bids: BTreeMap<Price, VecDeque<Order>>, // descending order if needed
    asks: BTreeMap<Price, VecDeque<Order>>, // ascending order
    order_index: HashMap<OrderID, Order>,
}


impl OrderBook {
    fn match_order(&mut self, mut order: Order, accounts: &mut HashMap<AccountID, Bankroll>) -> Vec<EngineMessage> {
        let mut fills = Vec::new();
        // Handle Stop orders
        if let OrdType::Stop = order.order_type {
            match order.side {
                Side::Buy => {
                    if let Some((&best_ask_price, _)) = self.asks.iter().next() {
                        if best_ask_price < order.price {
                            // Not triggered yet, buffer order
                            return fills;
                        }
                    } else {
                        // No market price, cannot trigger
                        return fills;
                    }
                    // Triggered, convert to Market order for matching
                    order.order_type = OrdType::Market;
                }
                Side::Sell => {
                    if let Some((&best_bid_price, _)) = self.bids.iter().next_back() {
                        if best_bid_price > order.price {
                            // Not triggered yet, buffer order
                            return fills;
                        }
                    } else {
                        // No market price, cannot trigger
                        return fills;
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
                        if best_ask_price < order.price {
                            // Not triggered yet, buffer order
                            return fills;
                        }
                    } else {
                        // No market price, cannot trigger
                        return fills;
                    }
                    // Triggered, convert to Limit order for matching
                    order.order_type = OrdType::Limit;
                }
                Side::Sell => {
                    if let Some((&best_bid_price, _)) = self.bids.iter().next_back() {
                        if best_bid_price > order.price {
                            // Not triggered yet, buffer order
                            return fills;
                        }
                    } else {
                        // No market price, cannot trigger
                        return fills;
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
                        self.asks.keys().next().filter(|&p| order.price >= *p).cloned()
                    };
                    if let Some(price) = best_ask_price {
                        let queue = self.asks.get_mut(&price).unwrap();
                        while order.quantity > 0 && !queue.is_empty() {
                            if let Some(mut best_ask) = queue.pop_front() {
                                let trade_qty = order.quantity.min(best_ask.quantity);
                                // Emit fill for incoming (buy) order
                                fills.push(EngineMessage::OrderFilled {
                                    order_id: order.order_id,
                                    filled_quantity: trade_qty,
                                    remaining_quantity: order.quantity - trade_qty,
                                    price: price,
                                    instrument_id: order.instrument_id.clone(),
                                    client_id: order.sender_id.clone(),
                                });
                                // Emit fill for matched (sell) order
                                fills.push(EngineMessage::OrderFilled {
                                    order_id: best_ask.order_id,
                                    filled_quantity: trade_qty,
                                    remaining_quantity: best_ask.quantity - trade_qty,
                                    price: price,
                                    instrument_id: best_ask.instrument_id.clone(),
                                    client_id: best_ask.sender_id.clone(),
                                });
                                // --- Account updates for Buy ---
                                // Buyer: order.account_id, Seller: best_ask.account_id
                                // Buyer: deduct cash, increase position
                                if let Some(buyer_account) = accounts.get_mut(&order.account_id) {
                                    buyer_account.cash -= price * trade_qty as f64;
                                    buyer_account.positions
                                        .entry(order.instrument_id.clone())
                                        .and_modify(|pos| *pos += trade_qty)
                                        .or_insert(trade_qty);
                                }
                                // Seller: increase cash, decrease position
                                if let Some(seller_account) = accounts.get_mut(&best_ask.account_id) {
                                    seller_account.cash += price * trade_qty as f64;
                                    seller_account.positions
                                        .entry(best_ask.instrument_id.clone())
                                        .and_modify(|pos| *pos -= trade_qty)
                                        .or_insert(0);
                                }
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
                            return fills;
                        } else {
                            // Fully or partially matched, no further action needed
                            return fills;
                        }
                    }
                    TimeInForce::FillOrKill => {
                        // Fill or Kill: if not fully filled, discard entire order
                        if order.quantity > 0 {
                            // Rollback any partial fills by re-adding asks consumed
                            // Since we don't track partial fills separately, for simplicity, discard entire order without adding to book
                            return fills;
                        } else {
                            // Fully filled
                            return fills;
                        }
                    }
                    _ => {
                        // Other TIF: post remaining quantity to book
                        if order.quantity > 0 {
                            self.bids.entry(order.price).or_default().push_back(order.clone());
                            self.order_index.insert(order.order_id, order);
                        }
                    }
                }
            }
            Side::Sell => {
                while order.quantity > 0 {
                    let best_bid_price = if order.order_type == OrdType::Market {
                        self.bids.keys().next_back().cloned()
                    } else {
                        self.bids.keys().next_back().filter(|&p| order.price <= *p).cloned()
                    };
                    if let Some(price) = best_bid_price {
                        let queue = self.bids.get_mut(&price).unwrap();
                        while order.quantity > 0 && !queue.is_empty() {
                            if let Some(mut best_bid) = queue.pop_front() {
                                let trade_qty = order.quantity.min(best_bid.quantity);
                                // Emit fill for incoming (sell) order
                                fills.push(EngineMessage::OrderFilled {
                                    order_id: order.order_id,
                                    filled_quantity: trade_qty,
                                    remaining_quantity: order.quantity - trade_qty,
                                    price: price,
                                    instrument_id: order.instrument_id.clone(),
                                    client_id: order.sender_id.clone(),
                                });
                                // Emit fill for matched (buy) order
                                fills.push(EngineMessage::OrderFilled {
                                    order_id: best_bid.order_id,
                                    filled_quantity: trade_qty,
                                    remaining_quantity: best_bid.quantity - trade_qty,
                                    price: price,
                                    instrument_id: best_bid.instrument_id.clone(),
                                    client_id: best_bid.sender_id.clone(),
                                });
                                // --- Account updates for Sell ---
                                // Seller: order.account_id, Buyer: best_bid.account_id
                                // Seller: increase cash, decrease position
                                if let Some(seller_account) = accounts.get_mut(&order.account_id) {
                                    seller_account.cash += price * trade_qty as f64;
                                    seller_account.positions
                                        .entry(order.instrument_id.clone())
                                        .and_modify(|pos| *pos -= trade_qty)
                                        .or_insert(0);
                                }
                                // Buyer: deduct cash, increase position
                                if let Some(buyer_account) = accounts.get_mut(&best_bid.account_id) {
                                    buyer_account.cash -= price * trade_qty as f64;
                                    buyer_account.positions
                                        .entry(best_bid.instrument_id.clone())
                                        .and_modify(|pos| *pos += trade_qty)
                                        .or_insert(trade_qty);
                                }
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
                            return fills;
                        } else {
                            // Fully or partially matched, no further action needed
                            return fills;
                        }
                    }
                    TimeInForce::FillOrKill => {
                        // Fill or Kill: if not fully filled, discard entire order
                        if order.quantity > 0 {
                            // Rollback any partial fills by re-adding bids consumed
                            // Since we don't track partial fills separately, for simplicity, discard entire order without adding to book
                            return fills;
                        } else {
                            // Fully filled
                            return fills;
                        }
                    }
                    _ => {
                        // Other TIF: post remaining quantity to book
                        if order.quantity > 0 {
                            self.asks.entry(order.price).or_default().push_back(order.clone());
                            self.order_index.insert(order.order_id, order);
                        }
                    }
                }
            }
            _ => {}
        }
        fills
    }

    fn remove_order(&mut self, order_id: OrderID, accounts: &mut HashMap<AccountID, Bankroll>) -> bool {
        if let Some(order) = self.order_index.get(&order_id).cloned() {
            let queue_opt = match order.side {
                Side::Buy => self.bids.get_mut(&order.price),
                Side::Sell => self.asks.get_mut(&order.price),
                _ => None,
            };
            if let Some(queue) = queue_opt {
                if let Some(idx) = queue.iter().position(|o| o.order_id == order_id) {
                    queue.remove(idx);
                    if queue.is_empty() {
                        match order.side {
                            Side::Buy => { self.bids.remove(&order.price); }
                            Side::Sell => { self.asks.remove(&order.price); }
                            _ => {}
                        }
                    }
                    self.order_index.remove(&order_id);
                    // Refund cash or restore position on cancellation
                    match order.side {
                        Side::Buy => {
                            if let Some(account) = accounts.get_mut(&order.account_id) {
                                account.cash += order.price * order.quantity as f64;
                            }
                        }
                        Side::Sell => {
                            if let Some(account) = accounts.get_mut(&order.account_id) {
                                account.positions
                                    .entry(order.instrument_id.clone())
                                    .and_modify(|pos| *pos += order.quantity)
                                    .or_insert(order.quantity);
                            }
                        }
                        _ => {}
                    }
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Debug)]
struct Bankroll {
    pub cash: AccountBalance,
    pub positions: HashMap<InstrumentID, Quantity>, // instrument -> quantity
}

#[derive(Debug)]
pub struct Exchange {
    order_counter: OrderID,
    accounts: HashMap<AccountID, Bankroll>,
    books: HashMap<InstrumentID, OrderBook>,
}

impl Exchange {
    pub fn new() -> Self {
        Self {
            order_counter: 1,
            accounts: HashMap::new(),
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
                client_order_id,
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

                let unit_price = price.unwrap_or(Price::from(0.0));
                let total_cost = unit_price * quantity as f64;

                let account = self.accounts.entry(account_id.clone()).or_insert_with(|| Bankroll {
                    cash: Price::from(1000.0),
                    positions: HashMap::new(),
                });

                if account.cash < total_cost {
                    return Some(EngineMessage::OrderRejected {
                        reason: "Insufficient funds".to_string(),
                        client_id,
                    });
                }

                account.cash -= total_cost;

                let order_id = self.order_counter;
                self.order_counter += 1;

                let order = Order {
                    order_id: order_id.clone(),
                    client_order_id: client_order_id.unwrap_or("".to_string()),
                    send_timestamp: sending_time,
                    receive_timestamp: receiving_time,
                    price: price.unwrap_or(Price::from(0.0)),
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
                let mut responses = book.match_order(order, &mut self.accounts);
                responses.push(EngineMessage::OrderAccepted {
                    client_id,
                    order_id
                });
                // If any responses, return them as a batch (or just the first if Option)
                // Here, for compatibility, if only one response, return it, else log or batch
                // For now, return only first, or all in a Vec in future
                // For demonstration, return all as a LogEvent if multiple
                if responses.len() == 1 {
                    Some(responses.remove(0))
                } else if !responses.is_empty() {
                    // In real use, would return Vec<EngineMessage>. For now, just log all.
                    // This is a limitation of the Option<EngineMessage> return type.
                    // So we return the first, but in practice the caller should handle Vec<EngineMessage>.
                    Some(responses.remove(0))
                } else {
                    None
                }
            }
            EngineMessage::CancelOrder {
                sending_time,
                receiving_time,
                order_id,
                client_id,
                ..
            } => {
                // Extract sending_time and receiving_time at the beginning of the branch (future logic)
                let _sending_time = sending_time;
                let _receiving_time = receiving_time;
                for (_instrument, book) in &mut self.books {
                    let removed = book.remove_order(order_id, &mut self.accounts);
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
                client_id,
                ..
            } => {
                // Amend logic not implemented yet
                Some(EngineMessage::LogEvent {
                    client_id: Some(client_id),
                    message: "Amend not yet implemented".to_string(),
                })
            }
            EngineMessage::AdvanceTime { client_id, .. } => {

                // AdvanceTime logic not implemented yet
                Some(EngineMessage::LogEvent {
                    client_id: Some(client_id),
                    message: "AdvanceTime not yet implemented".to_string(),
                })
            }
            _ => Some(EngineMessage::LogEvent {
                client_id: None,
                message: "Unsupported message received".to_string(),
            }),
        }
    }
}