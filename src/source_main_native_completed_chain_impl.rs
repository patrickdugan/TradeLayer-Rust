fn native_main_completed_chain(
    receipt: &mut NativeMainSessionReceipt,
    previous: &NativeMainSessionHead,
    maximum: usize,
) -> Result<Vec<u8>, NativeMainSessionError> {
    if !matches!(receipt.status, NativeMainSessionStatus::Completed(_))
        || receipt.profile != RECEIPT_PROFILE
        || receipt.sequence != previous.commands
        || receipt.previous_head != previous.commitment
    {
        return Err(NativeMainSessionError::WrongHead);
    }
    let encoded = native_main_completed_logical_bytes(receipt, maximum)?;
    let mut chain = previous.commitment.to_vec();
    chain.extend_from_slice(&encoded);
    receipt.head = sha256(&chain);
    Ok(encoded)
}
