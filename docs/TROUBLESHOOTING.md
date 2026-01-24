# Troubleshooting Guide

This guide helps diagnose and resolve common issues with Remote Smartcard.

## Quick Diagnostics

Run these commands to quickly identify issues:

```bash
# Check if services are running
systemctl status rsc-server rsc-client pcscd

# Check for errors in logs
journalctl -u rsc-server --since "10 minutes ago" | grep -i error
journalctl -u rsc-client --since "10 minutes ago" | grep -i error

# Verify smartcard is detected locally (on client)
pcsc_scan

# Verify virtual reader exists (on server)
pcsc_scan
```

## Connection Issues

### Client Cannot Connect to Server

**Symptoms:**
```
ERROR Connection failed: Connection refused
ERROR Connection failed: Connection timed out
```

**Diagnostic Steps:**

1. **Verify server is running:**
```bash
# On server
systemctl status rsc-server
ss -tlnp | grep 8443
```

2. **Check network connectivity:**
```bash
# From client
ping server-hostname
nc -zv server-hostname 8443
```

3. **Check firewall:**
```bash
# On server (UFW)
sudo ufw status | grep 8443

# On server (firewalld)
sudo firewall-cmd --list-ports | grep 8443

# On server (iptables)
sudo iptables -L -n | grep 8443
```

**Solutions:**

- If server not listening: `sudo systemctl start rsc-server`
- If firewall blocking:
  ```bash
  sudo ufw allow 8443/tcp  # UFW
  sudo firewall-cmd --add-port=8443/tcp --permanent && sudo firewall-cmd --reload  # firewalld
  ```
- If DNS issue: Use IP address instead of hostname

### TLS Handshake Failures

**Symptoms:**
```
ERROR TLS error: certificate verify failed
ERROR TLS error: unknown CA
ERROR TLS error: certificate has expired
```

**Diagnostic Steps:**

1. **Verify certificates:**
```bash
# Check server certificate
openssl x509 -in /etc/rsc-server/certs/server.crt -text -noout

# Check if server cert is signed by CA
openssl verify -CAfile /etc/rsc-server/certs/ca.crt /etc/rsc-server/certs/server.crt

# Check certificate dates
openssl x509 -in /etc/rsc-server/certs/server.crt -dates -noout
```

2. **Test TLS connection:**
```bash
# From client
openssl s_client -connect server:8443 \
    -CAfile /etc/rsc-client/certs/ca.crt \
    -cert /etc/rsc-client/certs/client.crt \
    -key /etc/rsc-client/certs/client.key
```

3. **Check certificate CN/SAN:**
```bash
openssl x509 -in /etc/rsc-server/certs/server.crt -text -noout | grep -A1 "Subject Alternative Name"
```

**Solutions:**

- **Wrong CA**: Ensure client has the correct `ca.crt` from the server
- **Hostname mismatch**: Use `--tls-server-name` to override, or regenerate certificate with correct SANs
- **Expired certificate**: Regenerate certificates
- **Self-signed without CA**: Add server cert to client's trusted certs

### Reconnection Loop

**Symptoms:**
```
INFO Connected to server
WARN Connection lost, reconnecting in 1s...
INFO Connected to server
WARN Connection lost, reconnecting in 2s...
```

**Diagnostic Steps:**

1. **Check server logs for disconnection reason:**
```bash
journalctl -u rsc-server -f
```

2. **Check network stability:**
```bash
ping -c 100 server | grep -E "time=|loss"
mtr server
```

3. **Check server resources:**
```bash
# On server
top -b -n1 | head -20
free -h
```

**Solutions:**

- **Network instability**: Use more stable network, or increase heartbeat timeout
- **Server overloaded**: Check server resources, reduce connected clients
- **Firewall timeouts**: Some firewalls drop idle connections; heartbeat should prevent this

## Smartcard Issues

### No Readers Found on Server

**Symptoms:**
```bash
pcsc_scan
# Waiting for the first reader...
```

**Diagnostic Steps:**

1. **Check rsc-server is running and connected:**
```bash
systemctl status rsc-server
journalctl -u rsc-server | grep -i "session\|reader"
```

2. **Check vpcd status:**
```bash
ps aux | grep vpcd
ls -la /var/run/pcscd/
```

3. **Check pcscd status:**
```bash
systemctl status pcscd
```

**Solutions:**

- **Client not connected**: Verify rsc-client is running and connected
- **vpcd not started**: Check server logs for vpcd errors
- **pcscd stale**: Restart pcscd: `sudo systemctl restart pcscd`

### Card Not Present

**Symptoms:**
```bash
pcsc_scan
# Reader 0: Virtual PCD 00 00
#   Card state: Card removed
```

**Diagnostic Steps:**

1. **Verify card is inserted locally:**
```bash
# On client machine
pcsc_scan
```

2. **Check client logs:**
```bash
journalctl -u rsc-client | grep -i "card\|atr"
```

**Solutions:**

- **Card not inserted**: Insert smartcard into reader
- **Card not detected locally**: Check local pcscd, try reinserting card
- **Reader registration failed**: Restart rsc-client

### APDU Transmission Errors

**Symptoms:**
```
ERROR APDU failed: Card was reset
ERROR APDU failed: Communication error
```

**Diagnostic Steps:**

1. **Test locally first:**
```bash
# On client machine
pkcs11-tool --list-slots
```

2. **Check timing:**
```bash
# Add debug logging
rsc-client --log-level debug ...
```

**Solutions:**

- **Card removed during operation**: Keep card inserted
- **Timeout**: Increase operation timeout in application
- **Protocol error**: Check card compatibility, try power cycling

## Performance Issues

### High Latency

**Symptoms:**
- Operations taking several seconds
- Timeouts in applications

**Diagnostic Steps:**

1. **Measure network latency:**
```bash
ping server
# Look at average time
```

2. **Enable timing logs:**
```bash
rsc-server --log-level debug
rsc-client --log-level debug
```

**Solutions:**

| Latency | Recommendation |
|---------|----------------|
| < 50ms | Normal, no action needed |
| 50-100ms | Works, may need timeout adjustment |
| 100-200ms | Increase timeouts, consider VPN |
| > 200ms | Consider dedicated connection |

### Application Timeouts

**Symptoms:**
- SSH hangs then fails
- GPG times out
- Browser shows "waiting for smartcard"

**Solutions:**

Different applications have different timeout settings:

**SSH:**
```bash
# In ~/.ssh/config
Host *
    PKCS11Provider /usr/lib/opensc-pkcs11.so
    ConnectTimeout 30
```

**GPG:**
```bash
# In ~/.gnupg/scdaemon.conf
card-timeout 30
```

## Logging and Debugging

### Enable Debug Logging

```bash
# Server
rsc-server --log-level debug ...

# Client
rsc-client --log-level debug ...

# For maximum verbosity
rsc-server --log-level trace ...
```

### Log Locations

| Component | Location |
|-----------|----------|
| rsc-server (systemd) | `journalctl -u rsc-server` |
| rsc-client (systemd) | `journalctl -u rsc-client` |
| pcscd | `journalctl -u pcscd` |

### Capture Logs for Bug Reports

```bash
# Server
sudo journalctl -u rsc-server --since "1 hour ago" > rsc-server.log

# Client
sudo journalctl -u rsc-client --since "1 hour ago" > rsc-client.log

# System info
uname -a > system-info.txt
cat /etc/os-release >> system-info.txt
rsc-server --version >> system-info.txt
rsc-client --version >> system-info.txt
```

## Common Error Messages

| Error | Cause | Solution |
|-------|-------|----------|
| "Connection refused" | Server not running | Start rsc-server |
| "Certificate verify failed" | Wrong CA certificate | Use correct ca.crt |
| "No such reader" | Reader not registered | Check client connection |
| "Card not present" | Card removed or not detected | Insert card, check local pcscd |
| "Session expired" | Heartbeat failure | Automatic reconnect should handle |
| "APDU error 6A82" | File not found on card | Card-specific, check documentation |
| "APDU error 6982" | Security status not satisfied | Enter PIN first |

## Getting Help

If you can't resolve the issue:

1. **Check existing issues**: https://github.com/alun-hub/remote-smartcard/issues

2. **Create a new issue** with:
   - OS version (client and server)
   - rsc version (`--version`)
   - Smartcard type
   - Debug logs (sanitize sensitive data)
   - Steps to reproduce

3. **Join discussions**: https://github.com/alun-hub/remote-smartcard/discussions
