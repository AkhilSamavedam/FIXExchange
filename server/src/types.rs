use fefix::{Buffer, FixValue};
use ordered_float::OrderedFloat;

pub(crate) type OrderID = u64;

pub(crate) type ClOrdID = String;

#[derive(Debug, Clone)]
pub(crate) struct ClientID {
    comp_id: String,
    sub_id: Option<String>
}

impl ClientID {
    pub(crate) fn new(comp_id: String, sub_id: Option<String>) -> Self {
        Self { comp_id, sub_id }
    }
}

pub(crate) type InstrumentID = String;
pub(crate) type Quantity = u64;
pub(crate) type Price = OrderedFloat<f64>;
pub(crate) type AccountBalance = OrderedFloat<f64>;

pub(crate) type AccountID = String;
