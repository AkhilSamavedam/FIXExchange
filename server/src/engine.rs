use fefix::definitions::fix50::*;
use fefix::fix_values::Timestamp;

use crate::types::*;

#[derive(Debug)]
pub enum EngineMessage {
    NewOrder {
        sending_time: Timestamp,
        receiving_time: Timestamp,
        client_id: ClientID,
        account_id: AccountID,
        client_order_id: Option<ClOrdID>,
        instrument_id: InstrumentID,
        order_type: OrdType,
        side: Side,
        quantity: Quantity,
        price: Option<Price>,
        time_in_force: Option<TimeInForce>,
    },
    CancelOrder {
        sending_time: Timestamp,
        receiving_time: Timestamp,
        client_id: ClientID,
        account_id: AccountID,
        order_id: OrderID
    },
    CreateInstrument {
        sending_time: Timestamp,
        receiving_time: Timestamp,
        instrument_id: InstrumentID,
    },
    AmendOrder {
        sending_time: Timestamp,
        receiving_time: Timestamp,
        order_id: OrderID,
        new_quantity: Option<Quantity>,
        new_price: Option<Price>,
        time_in_force: Option<TimeInForce>,
    },
    // Server -> Client responses
    OrderAccepted {
        client_id: ClientID,
        order_id: OrderID,
    },
    OrderRejected {
        reason: String,
        client_id: ClientID,
    },
    OrderFilled {
        client_id: ClientID,
        order_id: OrderID,
        filled_quantity: Quantity,
        remaining_quantity: Quantity,
        price: Price,
        instrument_id: InstrumentID,
    },
    OrderCancelled {
        client_id: ClientID,
        order_id: OrderID,
    },
    OrderAmended {
        client_id: ClientID,
        order_id: OrderID,
        new_quantity: Option<Quantity>,
        new_price: Option<Price>,
    },
    InvalidMessage {
        reason: String,
        raw_message: String,
    },
    // Data collection & backtesting
    Snapshot {
        timestamp: Timestamp,
        instrument_id: InstrumentID,
        bids: Vec<(Price, Quantity)>, // (price, quantity)
        asks: Vec<(Price, Quantity)>,
    },
    AdvanceTime {
        sending_time: Timestamp,
        receiving_time: Timestamp,
        timestamp: Timestamp,
    },
    LogEvent {
        message: String,
    },
}