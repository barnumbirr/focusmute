# Complete Actions Catalog

All actions discovered via MSVC RTTI symbols in the binary. These represent every user-triggerable and system-triggerable state change in the application.

## Input Channel Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `SetPreampGain` | scarlett | Set input preamp gain (dB) |
| `SetPhantomPower` | scarlett | Enable/disable 48V phantom power |
| `SetPhantomPowerPersistEnabled` | scarlett | Remember phantom power across restarts |
| `SetAir` | scarlett | Toggle Air mode on/off |
| `SetAirMode` | scarlett | Set Air mode type (presence/presence+drive) |
| `SetClipSafe` | scarlett | Toggle Clip Safe |
| `SetInputMode` | scarlett | Switch between line/inst |
| `SetImpedance` | scarlett | Set input impedance mode |
| `SetHighPassFilter` | scarlett | Toggle high-pass filter |
| `SetInsert` | scarlett | Toggle hardware insert |
| `SetDrive` | scarlett | Toggle drive effect |
| `SetConsoleEnabled` | scarlett | Toggle console emulation |
| `SetConsoleAmount` | scarlett | Set console emulation amount |
| `SetPad` | scarlett | Toggle input pad |
| `SetChannelLink` | scarlett | Toggle stereo link |

## Mixer Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `SetMixerLevel` | scarlett | Set mixer channel level (dB) |
| `SetMixerPan` | scarlett | Set mixer channel pan position |
| `SetMixerMute` | scarlett | Toggle mixer channel mute |
| `SetMixerSolo` | scarlett | Toggle mixer channel solo |
| `SetMixerSplitToMono` | scarlett | Split stereo to mono in mixer |
| `SetMixerChannelHidden` | scarlett | Hide/show mixer channel |
| `SetMixerMeteringMode` | scarlett | Switch metering display mode |
| `ShowAllMixerChannels` | scarlett | Unhide all mixer channels |
| `ResetAllPeakIndicators` | scarlett | Reset all peak meters |

## Output / Routing Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `SetOutputRouting` | scarlett | Set routing for an output |
| `SetOutputLevel` | scarlett, oca | Set output level |
| `SetOutputRoutingSource` | oca | Set output routing source (OCA layer) |
| `RemoveRouting` | scarlett | Remove a routing assignment |
| `SetSplitToMono` | scarlett | Split stereo output to mono |
| `RouteMixToMonitorGroup` | scarlett | Route a mix to monitor group |
| `UnrouteMixFromMonitorGroup` | scarlett | Remove mix from monitor group |
| `AddOutputChannelToMonitorGroup` | scarlett | Add output to monitor group |
| `RemoveOutputChannelFromMonitorGroup` | scarlett | Remove output from monitor group |
| `SetMonitorGroupChannelRoutingSource` | scarlett | Set routing source for monitor group channel |
| `SetMonitorGroupChannelTrim` | scarlett | Adjust monitor group channel trim |
| `SetActiveMonitorGroup` | scarlett | Switch active monitor group (speaker A/B) |

## Main Output Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `SetMainOutputLevel` | scarlett | Set main output volume |
| `SetMainOutputDim` | scarlett | Toggle dim mode |
| `SetMainOutputMute` | scarlett | Toggle mute |
| `SetMainOutputMono` | scarlett | Toggle mono mode |

## Auto Gain Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `StartAutoGain` | scarlett | Start auto gain on a channel |
| `CancelAutoGain` | scarlett | Cancel running auto gain |
| `AutoGainAll` | scarlett | Auto gain all channels |
| `EnterMultiAutoGainSelectionMode` | scarlett | Enter multi-channel selection |
| `SelectChannelForMultiChannelAutoGain` | oca | Select channel for multi auto gain |
| `DeselectChannelForMultiChannelAutoGain` | oca | Deselect channel |
| `SetSelectedMultiAutoGainChannels` | scarlett | Set selected channels |
| `StartMultiAutoGain` | scarlett | Start multi-channel auto gain |
| `StartMultiAutoGainAll` | scarlett | Start auto gain on all channels |
| `StartMultiChannelAutoGain` | oca | Start (OCA layer) |
| `CancelMultiAutoGain` | scarlett | Cancel multi auto gain |
| `RetryMultiAutoGain` | scarlett | Retry multi auto gain |
| `DismissMultiAutoGainResults` | scarlett | Dismiss results dialog |
| `DismissSingleAutoGainResults` | scarlett | Dismiss single results |
| `SabotageAutoGain` | scarlett | (Test/debug action) |

## Preset Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `SavePreset` | scarlett | Save current state as preset |
| `LoadPreset` | scarlett | Load a preset |
| `DeletePreset` | scarlett | Delete a preset |
| `RenamePreset` | scarlett | Rename a preset |
| `ExportPreset` | scarlett | Export preset to file |
| `ImportPresets` | scarlett | Import presets from file |
| `OverwritePreset` | scarlett | Overwrite existing preset |
| `ShowPresetConfirmationModal` | scarlett | Show preset confirmation dialog |
| `ShowRenamePresetModal` | scarlett | Show rename dialog |
| `NotifyPresetImportFailed` | scarlett | Notify import failure |

## Device Settings Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `SetSampleRate` | scarlett | Change sample rate |
| `SetDigitalIoMode` | scarlett | Set S/PDIF/ADAT mode |
| `SetInterfaceMode` | scarlett | Set USB interface mode |
| `InitiateDigitalIoModeUpdate` | scarlett | Initiate digital IO change |
| `InitiateInterfaceModeUpdate` | scarlett | Initiate interface mode change |
| `SetDirectMonitorEnabled` | scarlett | Toggle direct monitoring |
| `SetDirectMonitorMode` | scarlett | Set direct monitor mode |
| `SetLoopbackMirrorsDirectMonitorMix` | scarlett | Mirror direct monitor to loopback |
| `SetLedBrightness` | scarlett | Set LED brightness |
| `SetLedSleep` | scarlett | Set LED sleep behavior |
| `SetVideoCallModeEnabled` | scarlett | Toggle video call mode |
| `SetTalkbackActive` | scarlett | Toggle talkback |
| `SetTalkbackDestinations` | scarlett | Set talkback destination routing |
| `SetCustomNames` | scarlett | Set custom channel names |
| `SetMonoCustomName` | scarlett | Set mono channel name |
| `SetStereoCustomName` | scarlett | Set stereo channel name |
| `ClearCustomNames` | scarlett | Clear all custom names |

## Device Lifecycle Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `ConnectVirtualDevice` | scarlett | Connect virtual device (testing) |
| `DisconnectVirtualDevice` | scarlett | Disconnect virtual device |
| `SwitchDevice` | scarlett | Switch to different device |
| `NotifyDeviceConnectionError` | scarlett | Device connection error |
| `NotifyDeviceSwitched` | scarlett | Device was switched |
| `Initiate@factoryReset` | scarlett | Begin factory reset |
| `Confirm@factoryReset` | scarlett | Confirm factory reset |
| `Clear@factoryReset` | scarlett | Clear factory reset state |
| `ConfirmWithForcedTimeout@factoryReset` | scarlett | Force confirm reset |
| `Cancel@factoryReset` | scarlett | Cancel factory reset |
| `Confirm@deviceRestart` | scarlett | Confirm device restart |
| `Cancel@deviceRestart` | scarlett | Cancel device restart |
| `ShowConfirmationDialog@deviceRestart` | scarlett | Show restart dialog |
| `ReportAdaFirmwareUpdateStatus` | scarlett | Report firmware update |
| `DismissDeviceFirmwareUpdatePage` | scarlett | Dismiss firmware page |
| `ResolveVirtualDeviceFirmwareUpdate` | scarlett | Resolve virtual FW update |
| `ResolveVirtualDeviceRestart` | scarlett | Resolve virtual restart |

## Remote Connection Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `RequestRemoteConnection` | oca | Request remote connection |
| `UpdateRemoteConnectionCount` | oca | Update remote client count |
| `VerifyQrData` | oca | Verify QR code authentication |
| `Initiate@removeRemoteDevice` | scarlett | Begin removing remote device |
| `Confirm@removeRemoteDevice` | scarlett | Confirm removal |
| `Cancel@removeRemoteDevice` | scarlett | Cancel removal |

## UI/Application Actions

| Action | Namespace | Description |
|--------|-----------|-------------|
| `BringAllWindowsToFront` | scarlett | Bring app windows to front |
| `CenterApplicationWindow` | scarlett | Center the app window |
| `MinimizeWindow` | scarlett | Minimize window |
| `NotifyApplicationWindowMoved` | scarlett | Window was moved |
| `NotifyApplicationWindowResized` | scarlett | Window was resized |
| `DismissWelcomePage` | scarlett | Dismiss welcome screen |
| `DismissLoadingPage` | scarlett | Dismiss loading screen |
| `DismissModalDialog` | scarlett | Dismiss modal |
| `DismissAnalyticsConsentPage` | scarlett | Dismiss analytics consent |
| `NotifyWebLinkFollowed` | scarlett | A web link was opened |
| `ShowEasyStartNotification` | scarlett | Show easy start |
| `NotifyEasyStartNotificationClosed` | scarlett | Easy start closed |
| `ShowRatingNotification` | scarlett | Show rating prompt |
| `NotifyRatingNotificationClosed` | scarlett | Rating prompt closed |
| `NotifyMdnsRegistrationFailure` | scarlett | mDNS failed |
| `NotifyMdnsRegistrationSuccess` | scarlett | mDNS succeeded |
| `NotifyOcaServerInitialisationFailure` | scarlett | OCA server failed |
| `TerminateNetworkThread` | scarlett | Shut down network |

---
[← Device Model](04-device-model.md) | [Index](README.md) | [File Formats →](06-file-formats.md)
