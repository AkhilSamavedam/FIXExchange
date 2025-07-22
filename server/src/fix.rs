use fefix::{prelude::*};
use fefix::tagvalue::{Decoder, Config};
use fefix::definitions::fix50::*;
use fefix::fix_values::Timestamp;

use crate::types::*;
use crate::engine::EngineMessage;

pub fn handle_fix_message(message: &str) -> EngineMessage {
    let dict = Dictionary::fix50();
    let mut decoder = Decoder::<Config>::new(dict);
    decoder.config_mut().set_separator(b'|');

    let msg = match decoder.decode(message) {
        Ok(msg) => msg,
        Err(e) => {
            return EngineMessage::InvalidMessage {
                reason: e.to_string(),
                raw_message: message.to_string(),
            };
        }
    };

    // Common fields
    let sender_comp_id = msg.fv::<&str>(SENDER_COMP_ID).unwrap_or("UNKNOWN");
    let sender_sub_id = msg.fv::<&str>(SENDER_SUB_ID).ok();
    let client_id = ClientID::new(sender_comp_id.to_string(), sender_sub_id.map(str::to_string));

    let sending_time = match msg.fv::<Timestamp>(SENDING_TIME) {
        Ok(ts) => ts,
        Err(e) => {
            return EngineMessage::InvalidMessage {
                reason: e.unwrap().to_string(),
                raw_message: message.to_string(),
            };
        }
    };

    let receiving_time = Timestamp::utc_now();

    // MsgType determines what we should parse
    let msg_type = match msg.fv::<&str>(MSG_TYPE) {
        Ok(t) => t,
        Err(e) => {
            return EngineMessage::InvalidMessage {
                reason: e.unwrap().to_string(),
                raw_message: message.to_string(),
            };
        }
    };

    match msg_type {
        "D" => {
            // New Order - Single
            let instrument_id: InstrumentID = match msg.fv::<&str>(SYMBOL) {
                Ok(id) => id.to_string(),
                Err(e) => {
                    return EngineMessage::InvalidMessage {
                        reason: e.unwrap().to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let side = match msg.fv::<Side>(SIDE) {
                Ok(s) => s,
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid Side".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let quantity = match msg.fv::<Quantity>(QUANTITY) {
                Ok(qty) => qty,
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid Quantity".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let order_type = match msg.fv::<OrdType>(ORD_TYPE) {
                Ok(ot) => ot,
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid OrdType".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let time_in_force = msg.fv::<TimeInForce>(TIME_IN_FORCE).ok();

            // Only parse price if order type requires it
            let price: Option<Price> = match order_type {
                OrdType::Limit | OrdType::StopLimit => match msg.fv::<f64>(PRICE) {
                    Ok(p) => Some(Price::from(p)),
                    Err(_) => {
                        return EngineMessage::InvalidMessage {
                            reason: "Missing or invalid Price for limit/stop-limit order.".to_string(),
                            raw_message: message.to_string(),
                        };
                    }
                },
                _ => None,
            };

            let account_id: AccountID = match msg.fv::<&str>(ACCOUNT) {
                Ok(id) => id.to_string(),
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid Account ID".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let client_order_id = msg.fv::<&str>(CL_ORD_ID).ok().map(|id| id.to_string());

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
                time_in_force
            }
        }
        "F" => {
            // Cancel Order
            let order_id = match msg.fv::<OrderID>(ORDER_ID) {
                Ok(id) => id,
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid ClOrdID".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let account_id = match msg.fv::<&str>(ACCOUNT) {
                Ok(id) => id.to_string(),
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid account ID".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            EngineMessage::CancelOrder {
                sending_time,
                receiving_time,
                client_id,
                account_id,
                order_id,
            }
        }
        "UCI" => {

            let sender_comp_id = msg.fv::<&str>(SENDER_COMP_ID).unwrap_or("UNKNOWN");
            let sender_sub_id = msg.fv::<&str>(SENDER_SUB_ID).ok();

            // Custom type: Create Instrument
            let instrument_id: InstrumentID = match msg.fv::<&str>(SYMBOL) {
                Ok(id) => id.to_string(),
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid Symbol".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            EngineMessage::CreateInstrument {
                client_id: ClientID::new(sender_comp_id.to_string(), sender_sub_id.map(str::to_string)),
                sending_time,
                receiving_time,
                instrument_id,
            }
        }
        "G" => {
            let sender_comp_id = msg.fv::<&str>(SENDER_COMP_ID).unwrap_or("UNKNOWN");
            let sender_sub_id = msg.fv::<&str>(SENDER_SUB_ID).ok();

            // Amend Order
            let order_id = match msg.fv::<OrderID>(ORDER_ID) {
                Ok(id) => id,
                Err(_) => {
                    return EngineMessage::InvalidMessage {
                        reason: "Missing or invalid Order ID".to_string(),
                        raw_message: message.to_string(),
                    };
                }
            };

            let new_quantity = msg.fv::<Quantity>(ORDER_QTY).ok();
            let new_price: Option<Price> = msg.fv::<f64>(PRICE).ok().map(|p| Price::from(p));
            let time_in_force = msg.fv::<TimeInForce>(TIME_IN_FORCE).ok();

            EngineMessage::AmendOrder {
                client_id: ClientID::new(sender_comp_id.to_string(), sender_sub_id.map(str::to_string)),
                sending_time,
                receiving_time,
                order_id,
                new_quantity,
                new_price,
                time_in_force,
            }
        }
        _ => {
            EngineMessage::InvalidMessage {
                reason: format!("Unhandled MsgType: {}", msg_type),
                raw_message: message.to_string(),
            }
        }
    }
}