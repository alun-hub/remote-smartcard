//! vpcd Test Tool
//!
//! Simulates vpcd for testing Remote Smartcard server.
//! Listens on a port and allows sending test commands.

use clap::Parser;
use std::io::{self, Write};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, debug, error};

/// vpcd command types
#[repr(u8)]
enum VpcdCommand {
    PowerOff = 0x00,
    PowerOn = 0x01,
    Reset = 0x02,
    GetAtr = 0x03,
    Apdu = 0x04,
}

/// vpcd Test Tool
#[derive(Parser, Debug)]
#[command(name = "vpcd-test")]
#[command(author, version, about = "Simulates vpcd for testing")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "35963")]
    port: u16,

    /// Address to bind to
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let args = Args::parse();
    let addr = format!("{}:{}", args.bind, args.port);

    info!("vpcd-test starting on {}", addr);
    info!("Waiting for rsc-server to connect...");

    let listener = TcpListener::bind(&addr).await?;

    loop {
        let (socket, peer) = listener.accept().await?;
        info!("Connection from {}", peer);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket).await {
                error!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(socket: TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("rsc-server connected! You can now send commands.");
    println!("\nCommands:");
    println!("  p  - Power On (get ATR)");
    println!("  r  - Reset");
    println!("  a  - Get ATR");
    println!("  s  - Send SELECT APDU (A4 04 00)");
    println!("  g  - Get Data APDU (CA 00 00)");
    println!("  h  - Send hex APDU (enter hex bytes)");
    println!("  o  - Power Off");
    println!("  q  - Quit");
    println!();

    // Spawn a task to read responses
    let (mut read_half, mut write_half) = socket.into_split();

    let response_task = tokio::spawn(async move {
        loop {
            // Read 2-byte length
            let mut len_buf = [0u8; 2];
            match read_half.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) => {
                    debug!("Read error (connection closed?): {}", e);
                    break;
                }
            }

            let len = u16::from_be_bytes(len_buf) as usize;
            if len == 0 {
                println!("Response: (empty)");
                continue;
            }

            let mut data = vec![0u8; len];
            if let Err(e) = read_half.read_exact(&mut data).await {
                error!("Failed to read response data: {}", e);
                break;
            }

            // Pretty print the response
            if data.len() >= 2 {
                let sw1 = data[data.len() - 2];
                let sw2 = data[data.len() - 1];
                let body = &data[..data.len() - 2];

                if body.is_empty() {
                    println!("Response: SW={:02X}{:02X}", sw1, sw2);
                } else {
                    println!("Response: {} bytes, SW={:02X}{:02X}", body.len(), sw1, sw2);
                    println!("  Data: {}", hex::encode(body));
                }
            } else {
                println!("Response: {}", hex::encode(&data));
            }
            print!("> ");
            io::stdout().flush().ok();
        }
    });

    // Main input loop
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        let cmd = match input.chars().next().unwrap() {
            'p' | 'P' => {
                println!("Sending: Power On");
                Some((VpcdCommand::PowerOn, vec![]))
            }
            'r' | 'R' => {
                println!("Sending: Reset");
                Some((VpcdCommand::Reset, vec![]))
            }
            'a' | 'A' => {
                println!("Sending: Get ATR");
                Some((VpcdCommand::GetAtr, vec![]))
            }
            's' | 'S' => {
                // SELECT command (ISO 7816-4)
                // A4 04 00 00 - SELECT by name
                let apdu = vec![0x00, 0xA4, 0x04, 0x00, 0x00];
                println!("Sending: SELECT APDU: {}", hex::encode(&apdu));
                Some((VpcdCommand::Apdu, apdu))
            }
            'g' | 'G' => {
                // GET DATA command
                // CA 00 00 00 - Get data
                let apdu = vec![0x00, 0xCA, 0x00, 0x00, 0x00];
                println!("Sending: GET DATA APDU: {}", hex::encode(&apdu));
                Some((VpcdCommand::Apdu, apdu))
            }
            'h' | 'H' => {
                println!("Enter APDU in hex (e.g., 00A4040000): ");
                print!("hex> ");
                io::stdout().flush()?;

                let mut hex_input = String::new();
                io::stdin().read_line(&mut hex_input)?;
                let hex_input = hex_input.trim();

                match hex::decode(hex_input) {
                    Ok(apdu) => {
                        println!("Sending: APDU: {}", hex::encode(&apdu));
                        Some((VpcdCommand::Apdu, apdu))
                    }
                    Err(e) => {
                        println!("Invalid hex: {}", e);
                        None
                    }
                }
            }
            'o' | 'O' => {
                println!("Sending: Power Off");
                Some((VpcdCommand::PowerOff, vec![]))
            }
            'q' | 'Q' => {
                println!("Quitting...");
                break;
            }
            _ => {
                println!("Unknown command: {}", input);
                None
            }
        };

        if let Some((cmd, payload)) = cmd {
            // Build and send the command
            let mut data = vec![cmd as u8];
            data.extend_from_slice(&payload);

            let len = data.len() as u16;
            let len_bytes = len.to_be_bytes();

            write_half.write_all(&len_bytes).await?;
            write_half.write_all(&data).await?;
            write_half.flush().await?;

            debug!("Sent {} bytes", data.len());
        }
    }

    response_task.abort();
    Ok(())
}
