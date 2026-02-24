# Application Architecture

## Design Pattern: Redux-like Action/Dispatcher

Focusrite Control 2 uses a unidirectional data flow architecture similar to Redux:

```
User Interaction
      |
      v
  [Action]  ──>  [ActionDispatcher]  ──>  [Mediators]  ──>  [Model]
                                                               |
                                                               v
                                                         [JUCE UI Components]
```

### Key Components

1. **Actions** (`actions@scarlett` namespace) - Immutable data objects describing state changes
2. **ActionDispatcher** (`ActionDispatcher@focusrite`) - Central dispatch hub
3. **Mediators** - Business logic handlers (DeviceMediator, RoutingMediator, etc.)
4. **Model** (`Model@scarlett`) - Central application state with listener pattern
5. **Aes70Server** - Bridge between app state and device communication

## Namespace Hierarchy

```
focusrite::                     # Top-level namespace
  Action                        # Base action type
  ActionDispatcher              # Central dispatcher
  Animation                     # UI animations

scarlett::                      # Scarlett-specific logic
  Model                         # Application state model
  actions::                     # All user/system actions
    SetPreampGain
    SetPhantomPower
    SetAir / SetAirMode
    SetMixerLevel / SetMixerPan / SetMixerMute / SetMixerSolo
    SetOutputRouting
    SavePreset / LoadPreset / DeletePreset
    ...
  DeviceMediator                # Device state management
  RoutingList                   # Routing UI component
  LoadPresetDialog              # Preset loading UI
  Button                        # Custom button component
  Divider                       # UI divider component

ada::                           # Device abstraction layer ("Ada")
  Device                        # Abstract device interface
  Discovery                     # Device discovery
  Routing / SourceId / DestinationId  # Routing model
  MixId / MeterLevel            # Mixer model
  MonitorGroupEntry             # Monitor group model

oca::focusrite::                # OCA protocol layer
  InputChannel                  # OCA input channel representation
  Mix                           # OCA mix representation
  DeviceControlAdapter          # Bridge OCA <-> app model
  AuthenticationAgent_impl      # Remote auth handling

aes70::                         # AES70 protocol implementation
  dynamic_device                # Dynamic OCA device model
  device                        # Base device abstraction

fcp::ada::                      # Focusrite Control Protocol over Ada
  Context                       # Operation context
  PropertyBehaviour             # Property get/set behaviors
  FirmwareBehaviour             # Firmware update handling
```

## Server Architecture

> **CORRECTION**: The diagram below reflects the initial analysis based on RTTI symbols. Later findings (docs 10-12) proved that FC2 communicates with the local USB device via **FocusriteUsbSwRoot.sys IOCTLs** (TRANSACT protocol), NOT via the AES70/OCA endpoints. The AES70 endpoints are OCA servers that FC2 exposes for remote clients (e.g., Focusrite Control mobile app). See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for the actual device communication protocol.

The `Aes70Server` class is the central hub connecting:
- The `Model@scarlett` (app state)
- The `ActionDispatcher` (action handling)
- The `Aes70Device` (OCA device representation)
- Network endpoints (`Aes70SecureEndpoint`, `Aes70InsecureEndpoint`)

### Connection Flow (Remote Control Only)

```
[Remote Client]                 [Focusrite Control 2 App]
    |                                    |
    v                              [FocusritePal64.dll]
[Aes70Secure / Insecure                 |
 Endpoint]                              v
    |                          [FocusriteUsbSwRoot.sys]
    v                                    |
[Aes70Server ←──state sync──→ Model]    v
                                    [USB Device]
```

## State Model

The `Model@scarlett` maintains:
- Connected device information
- Input channel states (gain, phantom, air, impedance, etc.)
- Output channel states (routing, levels)
- Mixer state (per-mix channel levels, pan, mute, solo)
- Monitor groups and routing
- Preset metadata
- UI state (window position, size, page visibility)

---
[← Technology Stack](01-technology-stack.md) | [Index](README.md) | [AES70/OCA Protocol →](03-protocol-aes70.md)
