pub struct Invoice {
    pub total: u64,
    pub items: u32,
}

pub fn invoice(unit_price: u32, quantity: u32) -> Invoice {
    let total = u64::from(unit_price) * u64::from(quantity);
    let invoice = Invoice {
        total,
        items: quantity,
    };
    invoice
}
