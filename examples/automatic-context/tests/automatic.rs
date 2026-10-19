// A literal qualified invocation at the integration-test crate root.
// No Invoice import or manual declaration range is supplied.
checkmate::extract_fn!(config = "metadata/snippets.toml", snippet = "invoice");

checkmate::extract_fn! {
    fn_sig = "fn invoice_fields(total: u64, quantity: u32) -> (u64, u32)",
    module = "src/lib.rs",
    start = 8,
    end = 11,
    output = (invoice.total, invoice.items),
    snapshot = "snapshots/invoice.rs",
    context = "auto",
}

#[test]
fn returned_value_is_the_original_library_type() {
    let value: checkmate_auto_example::Invoice = extracted_invoice(12, 4);
    assert_eq!((value.total, value.items), (12, 4));
}

#[test]
fn supplied_inputs_do_not_recompute_the_earlier_initializer() {
    let value = extracted_invoice(999, 0);
    assert_eq!((value.total, value.items), (999, 0));
    assert_eq!(checkmate_auto_example::invoice(7, 0).total, 0);
}

#[test]
fn original_execution_and_matching_extracted_inputs_agree() {
    for (price, quantity) in [(0, 3), (3, 4), (u32::MAX, 2)] {
        let original = checkmate_auto_example::invoice(price, quantity);
        let extracted = extracted_invoice(u64::from(price) * u64::from(quantity), quantity);
        assert_eq!(
            (extracted.total, extracted.items),
            (original.total, original.items)
        );
    }
}

#[test]
fn inline_output_can_return_a_tuple_instead() {
    assert_eq!(invoice_fields(12, 4), (12, 4));
}

// Read-only inspection. Temporarily omit context_snapshot in the configuration
// to inspect an intentional update; review/copy the TOML yourself, then restore it.
#[test]
#[ignore]
fn print_context_for_review() {
    println!(
        "--- CONTEXT BEGIN ---\n{}--- CONTEXT END ---",
        invoice_context()
    );
}
