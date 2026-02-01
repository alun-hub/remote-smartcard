//! PC/SC smartcard reader interface
//!
//! Provides functions to interact with local smartcard readers via PC/SC.
//! Maintains persistent card connections to preserve applet selection state.

#![allow(dead_code)]

use pcsc::{Card, Context, Protocols, Scope, ShareMode};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info, trace, warn};

use crate::error::{ClientError, Result};

/// Global card connection cache
/// Maps reader name to active card connection
static CARD_CONNECTIONS: Mutex<Option<CardConnectionManager>> = Mutex::new(None);

/// Manages persistent card connections
struct CardConnectionManager {
    context: Context,
    connections: HashMap<String, Card>,
}

impl CardConnectionManager {
    fn new() -> Result<Self> {
        let context = Context::establish(Scope::User)?;
        Ok(Self {
            context,
            connections: HashMap::new(),
        })
    }

    fn get_or_connect(&mut self, reader_name: &str) -> Result<&Card> {
        // Check if we have an existing connection
        if !self.connections.contains_key(reader_name) {
            info!("Creating new card connection for reader: {}", reader_name);
            let reader_cstr = std::ffi::CString::new(reader_name)
                .map_err(|e| ClientError::Config(format!("Invalid reader name: {}", e)))?;

            // Try to connect with retry for busy card
            let card = self.connect_with_retry(&reader_cstr)?;

            self.connections.insert(reader_name.to_string(), card);
        } else {
            trace!("Reusing existing card connection for reader: {}", reader_name);
        }

        Ok(self.connections.get(reader_name).unwrap())
    }

    /// Connect to card with retry logic for busy/sharing violation errors
    fn connect_with_retry(&self, reader_cstr: &std::ffi::CString) -> Result<Card> {
        const MAX_RETRIES: u32 = 5;
        const RETRY_DELAY_MS: u64 = 500;

        for attempt in 1..=MAX_RETRIES {
            match self.context.connect(reader_cstr, ShareMode::Shared, Protocols::ANY) {
                Ok(card) => return Ok(card),
                Err(pcsc::Error::SharingViolation) => {
                    if attempt < MAX_RETRIES {
                        warn!(
                            "Card is busy (attempt {}/{}), retrying in {}ms...",
                            attempt, MAX_RETRIES, RETRY_DELAY_MS
                        );
                        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                    } else {
                        warn!("Card is busy, max retries reached");
                        return Err(ClientError::Smartcard(
                            "Card is busy (sharing violation). Another application may be using it.".to_string()
                        ));
                    }
                }
                Err(pcsc::Error::ReaderUnavailable) => {
                    if attempt < MAX_RETRIES {
                        warn!(
                            "Reader unavailable (attempt {}/{}), retrying in {}ms...",
                            attempt, MAX_RETRIES, RETRY_DELAY_MS
                        );
                        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                    } else {
                        return Err(ClientError::Smartcard(
                            "Reader unavailable after retries".to_string()
                        ));
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }

        // Should not reach here, but just in case
        Err(ClientError::Smartcard("Connection failed".to_string()))
    }

    fn disconnect(&mut self, reader_name: &str) {
        if self.connections.remove(reader_name).is_some() {
            info!("Disconnected from reader: {}", reader_name);
        }
    }

    fn transmit(&mut self, reader_name: &str, apdu: &[u8]) -> Result<Vec<u8>> {
        // Try to transmit, reconnecting if necessary
        let result = self.try_transmit(reader_name, apdu);

        match result {
            Ok(response) => Ok(response),
            Err(e) => {
                // If transmission failed, try reconnecting once
                warn!("APDU transmission failed, reconnecting: {}", e);
                self.disconnect(reader_name);
                self.try_transmit(reader_name, apdu)
            }
        }
    }

    fn try_transmit(&mut self, reader_name: &str, apdu: &[u8]) -> Result<Vec<u8>> {
        let card = self.get_or_connect(reader_name)?;

        // Prepare response buffer (extended APDU support)
        let mut response = vec![0u8; 65538]; // Max extended APDU response

        let response_len = card.transmit(apdu, &mut response)?.len();
        response.truncate(response_len);

        trace!("APDU response ({} bytes): {}", response.len(), hex::encode(&response));

        Ok(response)
    }

    fn get_atr(&mut self, reader_name: &str) -> Result<Vec<u8>> {
        let card = self.get_or_connect(reader_name)?;

        let mut atr_buf = [0u8; pcsc::MAX_ATR_SIZE];
        let mut reader_names_buf = [0u8; 256];

        let status = card.status2(&mut reader_names_buf, &mut atr_buf)?;

        let atr = status.atr().to_vec();
        debug!("Got ATR ({} bytes): {}", atr.len(), hex::encode(&atr));

        Ok(atr)
    }
}

/// Initialize the connection manager (call once at startup)
fn ensure_manager() -> Result<()> {
    let mut guard = CARD_CONNECTIONS.lock().unwrap();
    if guard.is_none() {
        *guard = Some(CardConnectionManager::new()?);
    }
    Ok(())
}

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
    ensure_manager()?;

    let mut guard = CARD_CONNECTIONS.lock().unwrap();
    let manager = guard.as_mut().unwrap();

    manager.get_atr(reader_name)
}

/// Transmit an APDU command to the card and return the response
/// Maintains persistent connection to preserve applet selection state
pub fn transmit_apdu(reader_name: &str, apdu: &[u8]) -> Result<Vec<u8>> {
    ensure_manager()?;

    trace!("Transmitting APDU to {}: {}", reader_name, hex::encode(apdu));

    let mut guard = CARD_CONNECTIONS.lock().unwrap();
    let manager = guard.as_mut().unwrap();

    manager.transmit(reader_name, apdu)
}

/// Disconnect from a specific reader (e.g., when card is removed)
pub fn disconnect(reader_name: &str) {
    if let Ok(mut guard) = CARD_CONNECTIONS.lock() {
        if let Some(manager) = guard.as_mut() {
            manager.disconnect(reader_name);
        }
    }
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
