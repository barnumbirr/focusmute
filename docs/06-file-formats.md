# File Formats & Data Locations

## Application Data Locations

| Path | Purpose |
|------|---------|
| `C:\Program Files\Focusrite\Focusrite Control 2\` | Application install |
| `C:\Program Files\Focusrite\Drivers\` | USB/ASIO drivers |
| `%APPDATA%\Focusrite\Focusrite Control 2\` | User settings & presets |
| `%APPDATA%\Focusrite\Focusrite Control 2\Analytics\` | Analytics data |
| `%LOCALAPPDATA%\Focusrite\Notifier\` | Notifier service data |

## Settings File

**Path**: `%APPDATA%\Focusrite\Focusrite Control 2\settings.xml`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Settings>
  <Window position="-1309, 661"/>
  <ActivePresets x8219="Default"/>
  <RemoteConnection key="<REDACTED>">
    <RemoteClients/>
  </RemoteConnection>
  <Devices>
    <Device serialNumber="<REDACTED>" productId="33305">
      <CustomChannelNames>
        <Inputs>
          <MonoEntry inputId="128" customName=""/>
          <MonoEntry inputId="129" customName=""/>
          <StereoEntry customName="">
            <MonoEntry inputId="1536" customName=""/>
            <MonoEntry inputId="1537" customName=""/>
          </StereoEntry>
          <MonoEntry inputId="772" customName=""/>
          <MonoEntry inputId="773" customName=""/>
        </Inputs>
      </CustomChannelNames>
    </Device>
  </Devices>
  <DeviceWindowSize width="498" height="602" device="33305"/>
  <Feedback lastFeedbackPrompt="2025-12-13T11:34:27.649+01:00"/>
  <SkippedWarningModals>
    <overwritePreset/>
  </SkippedWarningModals>
</Settings>
```

### Settings Fields

| Element | Description |
|---------|-------------|
| `Window@position` | Last window position (x, y) |
| `ActivePresets@x[PID]` | Active preset name per device product ID |
| `RemoteConnection@key` | Ed25519/X25519 public key for remote connections |
| `RemoteClients` | Authorized remote control clients |
| `Devices/Device` | Per-device settings (by serial number) |
| `CustomChannelNames` | User-assigned channel names |
| `DeviceWindowSize` | Window dimensions per device |
| `Feedback@lastFeedbackPrompt` | Last time feedback was requested |
| `SkippedWarningModals` | Modals the user has dismissed permanently |

## Preset File Format

**Path**: `%APPDATA%\Focusrite\Focusrite Control 2\Presets\0x[PID]\[UUID].xml`

The preset directory is organized by product ID (hex). Each preset is a UUID-named XML file.

### Preset Structure

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Preset name="Default" version="4"
        dateCreated="20251103T174413.863+0100"
        dateModified="20260129T091445.472+0100">
  <Device productId="33305" ...>
    <Inputs>
      <InputGroup groupName="Preamps">
        <Input inputId="128" channelName="Analogue 1" ...>
          <Controls phantomPower="0" air="0" airMode="presence"
                    clipSafe="0" mode="line" preampGain="54.0" .../>
          <availableInputMode mode="line"/>
          <availableInputMode mode="inst"/>
        </Input>
        ...
      </InputGroup>
    </Inputs>
    <Outputs>...</Outputs>
    <RoutingSources/>
    <Routings>
      <Routing inputId="768" outputId="128"/>
      ...
    </Routings>
    <Mixer isAvailable="0">
      <Mix mixId="768" mixName="Direct Monitor" ...>
        <MixerChannelGroup groupName="Analogue">
          <MonoMixerChannel inputId="772" ...>
            <Controls level="0.0" mute="unmuted" solo="unsoloed" pan="-1.0"/>
          </MonoMixerChannel>
          ...
        </MixerChannelGroup>
      </Mix>
    </Mixer>
    <CustomChannelNames>...</CustomChannelNames>
  </Device>
</Preset>
```

### Preset Fields

| Field | Values | Description |
|-------|--------|-------------|
| `phantomPower` | 0/1 | 48V phantom power |
| `air` | 0/1 | Air mode enabled |
| `airMode` | presence, presence+drive | Air type |
| `clipSafe` | 0/1 | Clip Safe |
| `mode` | line, inst | Input mode |
| `preampGain` | 0.0-70.0 | Gain in dB |
| `level` | -128.0 to 0.0 | Mixer level in dB (-128 = off) |
| `pan` | -1.0 to 1.0 | Pan position (L to R) |
| `mute` | unmuted/muted | Mute state |
| `solo` | unsoloed/soloed | Solo state |

## Log File

**Path**: `%APPDATA%\Focusrite\Focusrite Control 2\fc2.log`

Simple text format with timestamps:
```
****************************************************************
13 Feb 2026 15:02:17 Application initialised: Focusrite Control 2 1.847.0.0 (Windows 11)
```

Logs application start/stop, firmware updates, and errors.

## Analytics

**Path**: `%APPDATA%\Focusrite\Focusrite Control 2\Analytics\analytics.xml`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Analytics isSessionActive="1">
  <UserPreferences userConsentsToAnalytics="0"/>
</Analytics>
```

A `user.id` file contains a unique anonymous identifier.

## Version History (from logs)

| Date | Version |
|------|---------|
| Oct 2025 | 1.670.0.0 |
| Nov 2025 | 1.703.0.0 |
| Dec 2025 | 1.762.0.0 |
| Jan 2026 | 1.825.0.0 |
| Feb 2026 | 1.847.0.0 (current) |

---
[← Actions Catalog](05-actions-catalog.md) | [Index](README.md) | [Direct USB Feasibility →](07-direct-usb-feasibility.md)
