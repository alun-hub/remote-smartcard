# Windows Client Setup Guide

This guide covers setting up rsc-client on Windows.

## Prerequisites

- Windows 10/11 or Windows Server 2016+
- A smartcard reader (USB) with a smartcard/Yubikey inserted
- Network access to the rsc-server

## Installation

### Option 1: Download Pre-built Binary

Download the latest `rsc-client.exe` from the releases page.

### Option 2: Build from Source

1. Install Rust: https://rustup.rs/
2. Install Visual Studio Build Tools with C++ workload
3. Clone and build:

```powershell
git clone https://github.com/alun-hub/remote-smartcard.git
cd remote-smartcard
cargo build --release -p rsc-client
# Binary is at: target\release\rsc-client.exe
```

## Certificate Setup

The client requires certificates for mTLS authentication with the server.

### Option A: File-based Certificates (Recommended)

1. Obtain certificates from your CA administrator:
   - `ca.crt` - CA certificate for server verification
   - `client.crt` - Your client certificate
   - `client.key` - Your client private key

2. Place them in a secure location:
```powershell
mkdir C:\certs\rsc
# Copy certificates to C:\certs\rsc\
```

3. Set permissions (PowerShell as Administrator):
```powershell
$acl = Get-Acl "C:\certs\rsc\client.key"
$acl.SetAccessRuleProtection($true, $false)
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    "$env:USERNAME", "Read", "Allow")
$acl.AddAccessRule($rule)
Set-Acl "C:\certs\rsc\client.key" $acl
```

### Option B: Windows Certificate Store (Listing Only)

> **Note**: Full mTLS with Windows Certificate Store is not yet fully implemented.
> You can list certificates, but for actual authentication, use file-based certs.

1. Import your certificate (with private key):
```powershell
# Import .pfx file
certutil -user -importPFX "C:\path\to\client.pfx"

# Or double-click the .pfx file and follow the wizard
```

2. List available certificates:
```powershell
rsc-client.exe --list-windows-certs
```

3. Export to PEM files for use with rsc-client:
```powershell
# Export certificate
certutil -user -exportPFX -p "password" my "client.pfx"

# Convert with OpenSSL (install from https://slproweb.com/products/Win32OpenSSL.html)
openssl pkcs12 -in client.pfx -out client.crt -clcerts -nokeys
openssl pkcs12 -in client.pfx -out client.key -nocerts -nodes
```

## Configuration

### Method 1: Command Line Arguments

```powershell
rsc-client.exe `
  --server https://server.example.com:8443 `
  --tls-ca C:\certs\rsc\ca.crt `
  --tls-cert C:\certs\rsc\client.crt `
  --tls-key C:\certs\rsc\client.key `
  --tls-server-name server.example.com
```

### Method 2: Configuration File (Recommended)

1. Generate example config:
```powershell
rsc-client.exe --generate-config > C:\certs\rsc\config.toml
```

2. Edit `config.toml`:
```toml
[server]
url = "https://server.example.com:8443"

[tls]
ca_cert = "C:\\certs\\rsc\\ca.crt"
client_cert = "C:\\certs\\rsc\\client.crt"
client_key = "C:\\certs\\rsc\\client.key"
server_name = "server.example.com"

[client]
# client_id = "my-windows-pc"  # Optional, defaults to hostname
reconnect_delay = 1
reconnect_max_delay = 60

[logging]
level = "info"
```

3. Run with config:
```powershell
rsc-client.exe --config C:\certs\rsc\config.toml
```

## Running as a Service

### Option 1: Task Scheduler (Simple)

1. Open Task Scheduler (taskschd.msc)
2. Create Basic Task:
   - Name: "RSC Client"
   - Trigger: "At log on"
   - Action: Start a program
   - Program: `C:\path\to\rsc-client.exe`
   - Arguments: `--config C:\certs\rsc\config.toml`
3. In Properties, check "Run whether user is logged on or not"

### Option 2: NSSM (Non-Sucking Service Manager)

1. Download NSSM: https://nssm.cc/download
2. Install service:
```powershell
nssm install RSCClient C:\path\to\rsc-client.exe
nssm set RSCClient AppParameters "--config C:\certs\rsc\config.toml"
nssm set RSCClient DisplayName "Remote Smartcard Client"
nssm set RSCClient Description "Forwards local smartcard to remote server"
nssm set RSCClient Start SERVICE_AUTO_START
nssm start RSCClient
```

## Coexistence with Other Smartcard Software

rsc-client uses `SCARD_SHARE_SHARED` mode, which allows it to coexist with other
smartcard applications like:

- NetID Client
- Windows Hello for Business
- Other PIV/PKCS#11 applications

If the card is temporarily busy, rsc-client will automatically retry (up to 5 times
with 500ms delay between attempts).

### Troubleshooting Busy Card Issues

If you see "Card is busy (sharing violation)" errors:

1. Close any smartcard dialogs (PIN prompts, etc.)
2. Ensure no other application has exclusive access
3. Check if card operations are in progress

## Firewall Configuration

If using Windows Firewall, allow outbound connections:

```powershell
# PowerShell as Administrator
New-NetFirewallRule -DisplayName "RSC Client" `
  -Direction Outbound `
  -Program "C:\path\to\rsc-client.exe" `
  -Action Allow
```

## Logging

### View Logs

rsc-client logs to stdout by default. When running as a service:

```powershell
# With NSSM, logs go to configured output files
nssm get RSCClient AppStdout

# Or enable file logging in config.toml:
# [logging]
# file = "C:\\logs\\rsc-client.log"
```

### Debug Mode

For troubleshooting, enable debug logging:

```powershell
rsc-client.exe --config config.toml --log-level debug
```

## Verification

1. Check that your smartcard reader is detected:
```powershell
# PowerShell
Get-PnpDevice -Class SmartCardReader
```

2. Verify smartcard is accessible:
```powershell
certutil -scinfo
```

3. Run rsc-client and verify connection:
```powershell
rsc-client.exe --config config.toml --log-level debug
```

You should see:
```
INFO  Starting rsc-client v0.1.0
INFO  Found 1 local reader(s):
INFO    - Yubico YubiKey OTP+FIDO+CCID 00 00
INFO  Connecting to server: https://server.example.com:8443
INFO  Connected to server
INFO  Session established: <session-id>
INFO  Client ready - waiting for commands from server
```

## Common Issues

### "Certificate verify failed"

- Ensure `ca.crt` is the correct CA that signed the server certificate
- Verify server certificate has correct SAN (Subject Alternative Name)
- Use `--tls-server-name` to match the certificate's CN/SAN

### "Connection refused"

- Check firewall allows outbound connections to port 8443
- Verify server is running and accessible: `Test-NetConnection server.example.com -Port 8443`

### "Card is busy"

- Close PIN dialogs or other smartcard applications
- Wait a few seconds and try again
- rsc-client retries automatically up to 5 times

### "No readers found"

- Ensure smartcard reader is connected
- Check Device Manager for driver issues
- Install reader drivers if needed

## Uninstallation

### Remove Service (if installed with NSSM)
```powershell
nssm stop RSCClient
nssm remove RSCClient confirm
```

### Remove Files
```powershell
Remove-Item C:\path\to\rsc-client.exe
Remove-Item -Recurse C:\certs\rsc  # Be careful with certificates!
```

## Support

For issues specific to Windows:
- Ensure you have the latest Windows updates
- Check Event Viewer for related errors
- Enable debug logging and check output

For general issues, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
