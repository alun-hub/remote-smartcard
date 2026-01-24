# Remote Smartcard API Reference

This document describes the gRPC API used by Remote Smartcard for communication between client and server.

## Protocol Overview

Remote Smartcard uses gRPC over HTTP/2 with Protocol Buffers for serialization. The protocol supports both unary RPCs and bidirectional streaming.

## Service Definition

```protobuf
service RemoteSmartcard {
    // Session management
    rpc EstablishSession(SessionRequest) returns (SessionResponse);
    rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);

    // Reader management
    rpc RegisterReader(RegisterReaderRequest) returns (RegisterReaderResponse);
    rpc ListReaders(ListReadersRequest) returns (ListReadersResponse);

    // APDU transmission
    rpc Transmit(TransmitRequest) returns (TransmitResponse);

    // Command channel (bidirectional streaming)
    rpc CommandChannel(stream CommandResponse) returns (stream Command);
}
```

## Messages

### Session Management

#### SessionRequest

Establishes a new session with the server.

| Field | Type | Description |
|-------|------|-------------|
| client_id | string | Unique identifier for this client |
| client_version | string | Client software version |

#### SessionResponse

Response to session establishment.

| Field | Type | Description |
|-------|------|-------------|
| session_id | string | Assigned session ID (UUID) |
| server_version | string | Server software version |

#### HeartbeatRequest

Periodic heartbeat to maintain session.

| Field | Type | Description |
|-------|------|-------------|
| session_id | string | Current session ID |

#### HeartbeatResponse

Heartbeat response.

| Field | Type | Description |
|-------|------|-------------|
| session_valid | bool | Whether session is still valid |
| server_time | int64 | Server timestamp (Unix epoch) |

### Reader Management

#### RegisterReaderRequest

Registers a local reader with the server.

| Field | Type | Description |
|-------|------|-------------|
| session_id | string | Current session ID |
| reader_name | string | Local reader name |
| atr | bytes | Card ATR if present |
| card_present | bool | Whether card is inserted |

#### RegisterReaderResponse

Response to reader registration.

| Field | Type | Description |
|-------|------|-------------|
| success | bool | Registration success |
| virtual_reader_name | string | Name of virtual reader on server |
| error_message | string | Error description if failed |

#### ListReadersRequest

Request to list registered readers.

| Field | Type | Description |
|-------|------|-------------|
| session_id | string | Current session ID |

#### ListReadersResponse

List of registered readers.

| Field | Type | Description |
|-------|------|-------------|
| readers | ReaderInfo[] | Array of reader information |

#### ReaderInfo

Information about a single reader.

| Field | Type | Description |
|-------|------|-------------|
| name | string | Reader name |
| atr | bytes | Card ATR |
| card_present | bool | Card insertion status |

### APDU Transmission

#### TransmitRequest

Send an APDU command to the smartcard.

| Field | Type | Description |
|-------|------|-------------|
| session_id | string | Current session ID |
| reader_name | string | Target reader name |
| card_handle | uint64 | Card connection handle |
| apdu | bytes | APDU command bytes |

#### TransmitResponse

Response from APDU transmission.

| Field | Type | Description |
|-------|------|-------------|
| response | bytes | APDU response bytes |
| sw1 | uint32 | Status word 1 |
| sw2 | uint32 | Status word 2 |
| error_code | uint32 | Error code (0 = success) |
| error_message | string | Error description |

### Command Channel

The command channel is a bidirectional stream that allows the server to send commands to the client (e.g., APDU requests from vpcd).

#### Command

Server-to-client command.

| Field | Type | Description |
|-------|------|-------------|
| command_id | string | Unique command identifier |
| command_type | CommandType | Type of command |
| reader_name | string | Target reader |
| payload | bytes | Command-specific data |

#### CommandType

Enumeration of command types.

| Value | Description |
|-------|-------------|
| POWER_ON | Power on the card |
| POWER_OFF | Power off the card |
| RESET | Reset the card |
| GET_ATR | Get card ATR |
| TRANSMIT | Transmit APDU |

#### CommandResponse

Client-to-server response to a command.

| Field | Type | Description |
|-------|------|-------------|
| command_id | string | ID of command being responded to |
| success | bool | Command success |
| response | bytes | Response data |
| error_message | string | Error description |

## Connection Flow

### 1. Session Establishment

```
Client                                Server
   |                                    |
   |---- EstablishSession(req) -------->|
   |                                    |
   |<--- SessionResponse(session_id) ---|
   |                                    |
```

### 2. Reader Registration

```
Client                                Server
   |                                    |
   |---- RegisterReader(reader_info) -->|
   |                                    |
   |<--- RegisterReaderResponse --------|
   |                                    |
```

### 3. Command Channel

```
Client                                Server
   |                                    |
   |<==== CommandChannel (stream) =====>|
   |                                    |
   |<--- Command(TRANSMIT, apdu) -------|
   |                                    |
   |---- CommandResponse(response) ---->|
   |                                    |
```

### 4. Heartbeat Loop

```
Client                                Server
   |                                    |
   |---- Heartbeat(session_id) -------->|  (every 30s)
   |                                    |
   |<--- HeartbeatResponse -------------|
   |                                    |
```

## Error Handling

### gRPC Status Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| OK | Success | Operation completed successfully |
| INVALID_ARGUMENT | Bad request | Missing or invalid parameters |
| NOT_FOUND | Not found | Session or reader not found |
| UNAUTHENTICATED | Auth failed | TLS certificate invalid |
| UNAVAILABLE | Service down | Server not ready |
| INTERNAL | Server error | Unexpected server error |

### Application Error Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | Card not present |
| 2 | Reader not found |
| 3 | Transmission error |
| 4 | Card removed |
| 5 | Protocol error |

## TLS Configuration

### Server

```rust
ServerTlsConfig::new()
    .with_cert(cert_path)
    .with_key(key_path)
    .with_client_ca(ca_path)  // For mTLS
    .require_client_cert(true)  // For mTLS
```

### Client

```rust
ClientTlsConfig::new()
    .with_ca(ca_path)
    .with_client_cert(cert_path, key_path)  // For mTLS
    .with_server_name("server.example.com")
```

## Performance Considerations

### Recommended Timeouts

| Operation | Timeout |
|-----------|---------|
| Connection | 10 seconds |
| Heartbeat | 5 seconds |
| APDU Transmit | 30 seconds |
| Command Channel | No timeout (streaming) |

### Connection Pooling

The gRPC client maintains a connection pool. Recommended settings:

- Max connections: 1 (single server)
- Keep-alive interval: 30 seconds
- Keep-alive timeout: 20 seconds

## Example Usage

### Rust Client

```rust
use tonic::transport::Channel;
use rsc_protocol::remote_smartcard_client::RemoteSmartcardClient;

// Connect
let channel = Channel::from_static("https://server:8443")
    .tls_config(tls_config)?
    .connect()
    .await?;

let mut client = RemoteSmartcardClient::new(channel);

// Establish session
let response = client.establish_session(SessionRequest {
    client_id: "my-client".to_string(),
    client_version: "0.1.0".to_string(),
}).await?;

let session_id = response.into_inner().session_id;

// Register reader
client.register_reader(RegisterReaderRequest {
    session_id: session_id.clone(),
    reader_name: "Yubico YubiKey".to_string(),
    atr: vec![0x3B, 0x8D, ...],
    card_present: true,
}).await?;

// Start command channel
let (tx, rx) = mpsc::channel(100);
let response_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

let mut command_stream = client.command_channel(response_stream).await?
    .into_inner();

while let Some(command) = command_stream.message().await? {
    // Handle command
    let response = handle_command(command);
    tx.send(response).await?;
}
```

## Version Compatibility

| Client Version | Server Version | Compatible |
|----------------|----------------|------------|
| 0.1.x | 0.1.x | Yes |

Future versions will maintain backward compatibility within major versions.
