use fefix::{prelude::*};
use fefix::tagvalue::{Decoder, Config};
use fefix::definitions::fix50::*;
use fefix::fix_values::Timestamp;

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
            eprintln!("Invalid Sender Organization Number.");
            return;
        }
    };

    let sender_sub_id = match msg.fv::<&str>(SENDER_SUB_ID) {
        Ok(sub_id) => sub_id,
        _ => {
            eprintln!("Invalid Sender Sub ID.");
            return;
        }
    };

    let sender_timestamp = match msg.fv::<Timestamp>(SENDING_TIME) {
        Ok(sender_timestamp) => sender_timestamp,
        _ => {
            eprintln!("Invalid Sending Time.");
            return;
        }
    };

    let client_order_id = match msg.fv::<&str>(CL_ORD_ID) {
        Ok(id) => id,
        _ => {
            eprintln!("Invalid Client Order ID Number.");
            return;
        }
    };

    let account_id = match msg.fv::<&str>(ACCOUNT) {
        Ok(id) => id,
        _ => {
            eprintln!("Invalid Account ID.");
            return;
        }
    };

    let order_type = match msg.fv::<OrdType>(ORD_TYPE) {
        Ok(order_type) => order_type,
        _ => {
            eprintln!("Invalid Order Type.");
            return;
        }
    };

    let time_in_force = match msg.fv::<TimeInForce>(TIME_IN_FORCE) {
        Ok(time_in_force) => time_in_force,
        _ => {
            eprintln!("Invalid Time InForce.");
            return;
        }
    };

    let exec_instruction = match msg.fv::<ExecInst>(EXEC_INST) {
        Ok(exec_instruction) => exec_instruction,
        _ => {
            eprintln!("Invalid Execution Instruction.");
            return;
        }
    };

    let ticker = match msg.fv::<&str>(SYMBOL) {
        Ok(symbol) => symbol,
        _ => {
            eprintln!("Invalid Ticker.");
            return;
        }
    };

    let side = match msg.fv::<Side>(SIDE) {
        Ok(side) => side,
        _ => {
            eprintln!("Invalid or missing side field.");
            return;
        }
    };

    let quantity = match msg.fv::<u32>(QUANTITY) {
        Ok(qty) => qty,
        _ => {
            eprintln!("Missing order quantity.");
            return;
        }
    };

    let price = match msg.fv::<f64>(PRICE) {
        Ok(price) => price,
        _ => {
            eprintln!("Missing price.");
            return;
        }
    };

    let order = exchange.create_order(
        sender_timestamp,
        price,
        quantity,
        side,
        order_type,
        time_in_force,
        exec_instruction,
        ticker,
        account_id,
        sender_comp_id,
        sender_sub_id,
        client_order_id,
    );

    exchange.submit_order(order);
}