use fefix::{prelude::*};
use fefix::tagvalue::{Decoder, Config};
use fefix::definitions::fix50::*;

use crate::exchange::*;

pub fn handle_fix_message(exchange: &mut Exchange, message: &str) {
    let dict = Dictionary::fix50();
    let mut decoder = Decoder::<Config>::new(dict);
    decoder.config_mut().set_separator(b'|');

    let msg = match decoder.decode(message) {
        Ok(msg) => msg,
        Err(e) => {
            eprintln!("Failed to decode FIX message: {}", e);
            return;
        }
    };

    let sender_comp_id = match msg.fv::<&str>(SENDER_COMP_ID) {
        Ok(comp_id) => comp_id,
        _ => {
            eprintln!("Invalid Sender Organization Number");
            return;
        }
    };

    let sender_sub_id = match msg.fv::<&str>(SENDER_SUB_ID) {
        Ok(sub_id) => sub_id,
        _ => {
            eprintln!("");
            return;
        }
    };

    let client_order_id = match msg.fv::<&str>(CL_ORD_ID) {
        Ok(id) => id,
        _ => {
            eprintln!("Invalid Client Order ID Number");
            return;
        }
    };

    let side = match msg.fv::<Side>(SIDE) {
        Ok(side) => side,
        _ => {
            eprintln!("Invalid or missing side field");
            return;
        }
    };

    let quantity = match msg.fv::<u32>(QUANTITY) {
        Ok(qty) => qty,
        _ => {
            eprintln!("Missing order quantity");
            return;
        }
    };

    let price = match msg.fv::<f64>(PRICE) {
        Ok(price) => price,
        _ => {
            eprintln!("Missing price");
            return;
        }
    };

    let ticker = match msg.fv::<&str>(SYMBOL) {
        Ok(symbol) => symbol,
        _ => {
            eprintln!("Missing symbol");
            return;
        }
    };
}