
use crate::exchange::*;

pub fn handle_fix_message(exchange: &mut Exchange, message: &str) {
    let fields: std::collections::HashMap<_, _> = message
        .split('|')
        .filter_map(|kv| {
            let mut parts = kv.splitn(2, '=');
            Some((parts.next()?, parts.next()?))
        })
        .collect();

    match fields.get("35") {
        Some(&"D") => {
            // New Order Single
            let ticker = fields.get("55").unwrap().to_uppercase();
            let side = match fields.get("54") {
                Some(&"1") => Side::Buy,
                Some(&"2") => Side::Sell,
                _ => return,
            };
            let price = fields.get("44").and_then(|p| p.parse::<f64>().ok()).unwrap_or(0.0);
            let quantity = fields.get("38").and_then(|q| q.parse::<u32>().ok()).unwrap_or(0);
            let order = Order::new(side, price, quantity, &ticker);
            exchange.submit_order(order);
        }
        Some(&"F") => {
            // Cancel Order (if implemented)
            // Placeholder for cancel logic
        }
        _ => {
            eprintln!("Unknown or unsupported FIX message: {}", message);
        }
    }
}
