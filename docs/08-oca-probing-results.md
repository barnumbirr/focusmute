# OCA Server Probing Results

## Date: 2026-02-13

## FC2 Network Configuration

| Port | Bind | Protocol | Status |
|------|------|----------|--------|
| 58322 | 0.0.0.0 | TCP (HTTP) | LISTENING, all paths return 404, rejects WebSocket |
| 58323 | 0.0.0.0 | TCP (WebSocket) | LISTENING, accepts any WebSocket connection |

FC2 process PID: 2504

## Port 58322 (HTTP Server)

- Returns `HTTP/1.1 404 Not Found` for ALL tested paths
- Paths tested: /, /index.html, /api, /device, /oca, /ws, /websocket, /control, /aes70, /connect, /nmos, /x-nmos/ncp/v1.0/connect, /focusrite, /scarlett, /status, /info, /health, /.well-known, /device/info, /api/device
- Rejects all WebSocket upgrade attempts on all paths and subprotocols
- Accepts raw TCP connections but doesn't respond to OCP.1 binary data

## Port 58323 (WebSocket Server)

- Accepts WebSocket connections on **any path** with **any subprotocol**
- Behavior with different data types:

| Data Type | Result |
|-----------|--------|
| Binary OCP.1 with sync byte | **Connection closed immediately** |
| Binary without sync byte | **Connection closed immediately** |
| Raw OCA command | **Connection closed immediately** |
| IS-12 JSON commands | **Accepted, no response, connection stays open** |
| KeepAlive byte (0x04) | **Accepted, no response, connection stays open** |
| Arbitrary text | **Accepted, no response, connection stays open** |

### Key Observation
Binary data starting with specific byte patterns causes immediate disconnect, while JSON/text is silently absorbed. This suggests the server IS parsing the data and:
1. Rejecting invalid binary protocol messages by closing the connection
2. Buffering/ignoring text/JSON it doesn't understand
3. Possibly expecting **encrypted** binary data (the server may attempt to decrypt incoming binary, fail, and disconnect)

## IS-12 JSON Commands Tested (no response to any)

1. `NcObject.Get` for `NcBlock.members` property (oid=1)
2. `NcBlock.GetMemberDescriptors` with recurse=true (oid=1)
3. `NcObject.Get` for role property (oid=1)
4. Subscription to oid=1

## Raw TCP OCP.1 Tests (no response on either port)

Tested standard OCP.1 binary format on both ports:
- KeepAlive PDU: no response
- Command PDU (GetModelDescription on DeviceManager): no response

## Interpretation

Port 58323 is almost certainly the **Aes70SecureEndpoint** from the binary analysis. It accepts WebSocket connections but expects **libsodium secretstream encrypted** data. When it receives unencrypted binary, it attempts to decrypt/parse it, fails, and disconnects. Text/JSON doesn't trigger the binary parser so the connection stays open.

The authentication flow (from binary analysis) requires:
1. QR code exchange (`SendQRData`, `VerifyQrData`)
2. Client approval (`RequestApproval`, `Decision`)
3. Encrypted tunnel establishment (libsodium secretstream)
4. Then OCA commands inside the encrypted channel

Port 58322 may be the **Aes70InsecureEndpoint** but it appears to only serve HTTP content (all 404s suggest it expects a specific URL path that we haven't found, possibly advertised via mDNS to the Focusrite mobile app).

## Conclusion

**The OCA server cannot be used without reverse-engineering the authentication/encryption handshake.** The encryption uses libsodium secretstream (XChaCha20-Poly1305), and the authentication involves QR code-based pairing.

**Next step** (completed): USB traffic capture with Wireshark + USBPcap revealed the full protocol. LED halo control is fully working via SET_DESCR + DATA_NOTIFY through FocusriteUsbSwRoot.sys. See [09-led-control-api-discovery.md](09-led-control-api-discovery.md) and [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md).

---

## Addendum: FC2 Daemon IPC Protocol (XML-over-TCP)

> Source: [nickmorozov/FocusriteVolumeControl](https://github.com/nickmorozov/FocusriteVolumeControl) — experimental `FocusriteClient.swift` TCP client (not fully integrated). Protocol details extracted from source code, not verified against a live FC2 instance.

Separate from the encrypted OCA/WebSocket server on port 58323, the FC2 background daemon (`FocusriteControlServer`) exposes a **plaintext XML-over-TCP** IPC protocol on localhost. This is the protocol that third-party apps (e.g., Focusrite's own MIDI Control app) use to control the device without touching USB directly.

### Transport

- **TCP** on `127.0.0.1` (localhost only)
- **Port discovery**: `lsof -i TCP -s TCP:LISTEN -n -P` filtered for processes named "Focusrite"
- **Known ports**: 58323 (primary), 58322, 49152, 30096 (fallbacks)

> **NOTE**: Port 58323 is the same port we probed as a WebSocket server above. The XML-over-TCP protocol may be a separate listener, or FC2 may multiplex both protocols on the same port (accepting raw TCP for XML clients and WebSocket upgrades for OCA clients). This has not been verified.

### Message Framing

Every message (client→server and server→client) uses a 14-byte ASCII header:

```
Length=XXXXXX <xml-payload>
```

| Field | Size | Description |
|-------|------|-------------|
| `Length=` | 7 bytes | Literal prefix |
| `XXXXXX` | 6 bytes | Hex-encoded payload length, zero-padded, uppercase |
| ` ` | 1 byte | Space separator |
| payload | variable | UTF-8 XML |

The length field is the **character count** of the XML payload, not the byte count. For ASCII-only XML these are identical.

Example: a 42-character XML body → `Length=00002A <xml...>`

### Session Lifecycle

#### 1. Client registration

```xml
→ <client-details hostname="MyApp" client-key="987654321"/>
← <client-details id="ASSIGNED_ID" .../>
```

The `client-key` is a simple numeric string. The value `"987654321"` is taken from Focusrite's own MIDI Control app.

#### 2. Server approval

```xml
← <approval authorised="true"/>
```

Only after approval can the client issue `set` commands.

#### 3. Device arrival

```xml
← <device-arrival id="DEVICE_ID" model="Scarlett Solo" ...>
    <item id="ITEM_1" value="VALUE_1"/>
    <item id="ITEM_2" value="VALUE_2"/>
    ...
  </device-arrival>
```

Contains the full initial state of all device parameters as `<item>` elements. Items are addressed by **string IDs** with **string values** — the ID format is unknown (may be human-readable names, numeric offsets, or GUIDs).

#### 4. Subscription

```xml
→ <device-subscribe devid="DEVICE_ID" subscribe="true"/>
```

After subscribing, the server pushes value changes.

#### 5. Keep-alive

```xml
→ <keep-alive/>
```

Sent every **3 seconds** by the client.

#### 6. Set (write) values

```xml
→ <set devid="DEVICE_ID"><item id="ITEM_ID" value="NEW_VALUE"/></set>
```

#### 7. Value change notification (server push)

```xml
← <set devid="DEVICE_ID"><item id="ITEM_ID" value="NEW_VALUE"/></set>
```

Same format as the client write command, pushed to all subscribed clients when a parameter changes.

#### 8. Device removal

```xml
← <device-removal .../>
```

### Relationship to Our TRANSACT Protocol

This XML IPC protocol sits **above** the USB layer:

```
Third-party app  ──XML/TCP──►  FocusriteControlServer daemon  ──IOCTL/TRANSACT──►  Device
                               (FC2 background process)
```

The daemon translates XML item IDs and values into the SET_DESCR/GET_DESCR/DATA_NOTIFY commands we documented in [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) and [13-protocol-reference.md](13-protocol-reference.md). The mapping between XML item IDs and APP_SPACE descriptor offsets is internal to FC2.

### What We Don't Know

- The exact format of item IDs (human-readable? numeric? hierarchical?)
- Whether this protocol works on port 58323 alongside the WebSocket/OCA server, or on a different port
- Whether the `client-key` value matters or is just an opaque identifier
- Whether approval is automatic or requires user interaction (e.g., a dialog in FC2)
- Whether this protocol is available on Windows (our probing in this document was on Windows; the FocusriteVolumeControl project is macOS-only)

---
[← Direct USB Feasibility](07-direct-usb-feasibility.md) | [Index](README.md) | [LED Control API →](09-led-control-api-discovery.md)
