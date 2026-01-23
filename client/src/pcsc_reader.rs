//! PC/SC smartcard reader interface
//!
//! Provides functions to interact with local smartcard readers via PC/SC.

use pcsc::{Context, Protocols, Scope, ShareMode};
use tracing::{debug, trace};

use crate::error::{ClientError, Result};

/// List all available smartcard readers
pub fn list_readers() -> Result<Vec<String>> {
    let ctx = Context::establish(Scope::User)?;

    // Get the length of the readers buffer
    let readers_len = ctx.list_readers_len()?;

    if readers_len == 0 {
        return Ok(Vec::new());
    }

    // Allocate buffer and get reader names
    let mut readers_buf = vec![0u8; readers_len];
    let readers = ctx.list_readers(&mut readers_buf)?;

    let reader_names: Vec<String> = readers
        .map(|r| r.to_string_lossy().to_string())
        .collect();

    debug!("Found {} reader(s)", reader_names.len());
    Ok(reader_names)
}

/// Get the ATR (Answer To Reset) from a card in the specified reader
pub fn get_atr(reader_name: &str) -> Result<Vec<u8>> {
    let ctx = Context::establish(Scope::User)?;

    debug!("Connecting to reader: {}", reader_name);

    // Connect to the card
    let reader_cstr = std::ffi::CString::new(reader_name)
        .map_err(|e| ClientError::Config(format!("Invalid reader name: {}", e)))?;
    let card = ctx.connect(
        &reader_cstr,
        ShareMode::Shared,
        Protocols::ANY,
    )?;

    // Get card status which includes ATR
    let mut atr_buf = [0u8; pcsc::MAX_ATR_SIZE];
    let mut reader_names_buf = [0u8; 256];

    let status = card.status2(&mut reader_names_buf, &mut atr_buf)?;

    let atr = status.atr().to_vec();
    debug!("Got ATR ({} bytes): {}", atr.len(), hex::encode(&atr));

    Ok(atr)
}

/// Transmit an APDU command to the card and return the response
pub fn transmit_apdu(reader_name: &str, apdu: &[u8]) -> Result<Vec<u8>> {
    let ctx = Context::establish(Scope::User)?;

    trace!("Transmitting APDU to {}: {}", reader_name, hex::encode(apdu));

    let reader_cstr = std::ffi::CString::new(reader_name)
        .map_err(|e| ClientError::Config(format!("Invalid reader name: {}", e)))?;
    let card = ctx.connect(
        &reader_cstr,
        ShareMode::Shared,
        Protocols::ANY,
    )?;

    // Prepare response buffer
    let mut response = vec![0u8; 258]; // Max short APDU response + SW1SW2

    let response_len = card.transmit(apdu, &mut response)?.len();
    response.truncate(response_len);

    trace!("Got response ({} bytes): {}", response.len(), hex::encode(&response));

    Ok(response)
}

/// Check if a card is present in the specified reader
pub fn is_card_present(reader_name: &str) -> Result<bool> {
    let ctx = Context::establish(Scope::User)?;

    // Get reader state
    let reader_cstr = std::ffi::CString::new(reader_name)
        .map_err(|e| ClientError::Config(format!("Invalid reader name: {}", e)))?;
    let mut reader_states = vec![pcsc::ReaderState::new(
        reader_cstr,
        pcsc::State::UNAWARE,
    )];

    // Check current state (timeout 0 = immediate)
    ctx.get_status_change(Some(std::time::Duration::from_millis(0)), &mut reader_states)?;

    let state = reader_states[0].event_state();
    let present = state.contains(pcsc::State::PRESENT);

    debug!("Card present in {}: {}", reader_name, present);
    Ok(present)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires actual smartcard reader
    fn test_list_readers() {
        let readers = list_readers().unwrap();
        println!("Readers: {:?}", readers);
    }

    #[test]
    #[ignore] // Requires actual smartcard
    fn test_get_atr() {
        let readers = list_readers().unwrap();
        if let Some(reader) = readers.first() {
            let atr = get_atr(reader).unwrap();
            println!("ATR: {}", hex::encode(&atr));
        }
    }
}
