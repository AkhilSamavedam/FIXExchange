use std::fmt::Display;

use ordered_float::OrderedFloat;

pub(crate) type OrderID = u64;

pub(crate) type ClOrdID = String;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct ClientID {
    comp_id: String,
    sub_id: Option<String>
}

impl Display for ClientID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(sub_id) = &self.sub_id {
            Ok(write!(f, "{}::{}", self.comp_id, sub_id)?)
        }
        else {
            Ok(write!(f, "{}", self.comp_id)?)
        }
    }
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
