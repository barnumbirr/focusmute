# AES70/OCA Communication Protocol

## Overview

Focusrite Control 2 communicates with Scarlett 4th Gen devices using **AES70** (also known as **OCA - Open Control Architecture**), an open standard for media device control (AES standard AES70-2018).

## Protocol Stack

```
+----------------------------------+
| Focusrite Application Logic      |
| (scarlett::actions, Model)       |
+----------------------------------+
| Focusrite OCA Extensions         |
| (oca::focusrite namespace)       |
+----------------------------------+
| AES70 Dynamic Device Model       |
| (dynamic_device@aes70)           |
+----------------------------------+
| WebSocket Transport              |
| (websocket:: namespace)          |
+----------------------------------+
| Optional: secretstream encrypt   |
| (secretstream@aes70 - libsodium) |
+----------------------------------+
| TCP/IP (libuv)                   |
+----------------------------------+
```

## Two Endpoint Types

### 1. Aes70InsecureEndpoint
- OCA server exposed by FC2 for **local remote-control clients** (e.g., Focusrite Control mobile app on the same LAN)
- Plain WebSocket, no encryption
- mDNS service registration for discovery
- **Note**: FC2 itself does NOT use this endpoint to communicate with the USB device — it uses FocusriteUsbSwRoot.sys IOCTLs instead (see docs 10-12)

### 2. Aes70SecureEndpoint
- Used for **remote/network connections** (Focusrite Control on another device)
- WebSocket + libsodium secretstream encryption
- QR code-based authentication (`VerifyQrData`, `SendQRData` actions)
- Client authorization tracking (`updateCachedAuthorisedClients`)

## Network Configuration

- **Port Range**: TCP 58322-59321 (from firewall rule)
- **Firewall**: Inbound TCP rule for the port range on private/domain/public profiles
- **Description**: "Allows [app] to communicate with remote devices on your local network for remote control and device management"

## AES70 Object Model

The device is modeled using AES70 OCA classes:

- **OcaRoot** - Base class for all OCA objects
- **OcaAgent** - Agent objects (AuthenticationAgent, etc.)
- **dynamic_device** - Runtime-assembled device tree
- **dynamic_block** - Container blocks with dynamic children
- **generic_device_storage** - Device property storage
- **generic_subscription_storage** - Event subscription management

### Focusrite OCA Extensions

Focusrite extends the standard AES70 model with custom classes:

- `OcaFocusriteAuthenticationAgent` - Authentication for remote access
- `AuthenticationAgent_impl` - Implementation with `RequestApproval` and `SendQRData`
- `DynamicBlockAdapter` - Custom block adapter
- `DeviceControlAdapter` - Maps OCA properties to app-level controls
- `InputChannel@oca@focusrite` - Extended input channel with Focusrite-specific properties
- `Mix@oca@focusrite` - Extended mix representation

## Property System

The `fcp::ada` layer provides a property behavior system:

```cpp
PropertyBehaviour<Getter, Setter>
```

Properties are typed and support:
- Get/Set with error handling (`expected<T, Error>`)
- Availability checking (`ValueWithAvailability`)
- Options enumeration (`ValueWithOptions`)
- Context-dependent behavior

### Property Types Observed

- `Decibels` - Gain levels (preamp, mixer, output)
- `Hertz` - Sample rates
- `SourceId` / `DestinationId` - Routing endpoints
- `MixId` - Mix identifiers
- `MonitorGroup` - Monitor group selection
- `Enabled` - Boolean-like enable/disable
- `AirMode` - Air mode selection (presence, presence+drive, etc.)
- `ImpedanceMode` - Input impedance
- `ChannelLinkMode` - Stereo linking
- `DirectMonitorMode` - Direct monitor configuration
- `DigitalIoMode` - S/PDIF/ADAT optical port mode
- `InterfaceModeType` - USB interface mode
- `AutoGainState` - Auto gain state machine
- `MeterLevel` - Metering data

## Key AES70 Insight

> **UPDATE**: OCA access was not pursued. The secure endpoint requires libsodium secretstream authentication (key exchange unknown), and the insecure endpoint is non-functional for device control (returns 404 on all paths — see [08-oca-probing-results.md](08-oca-probing-results.md)). Device control is achieved through the TRANSACT protocol instead (see docs 10-12).

Because Focusrite uses AES70/OCA, it was theoretically possible to:
1. Write an alternative OCA client that connects to the device
2. Use OCA discovery to enumerate the device's object tree
3. Get/set properties and subscribe to changes via standard OCA mechanisms
4. The protocol is an open standard with public documentation

---
[← Architecture](02-architecture.md) | [Index](README.md) | [Device Model →](04-device-model.md)
