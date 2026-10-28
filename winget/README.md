# WinGet Package for Notch (`KrisnaSantosa15.notch`)

This folder contains the manifest for publishing Notch to the official [Windows Package Manager Community Repository](https://github.com/microsoft/winget-pkgs).

Once merged, anyone on Windows 10/11 can install Notch via:

```powershell
winget install KrisnaSantosa15.notch
```

---

## Validating the Manifest Locally

```powershell
winget validate --manifest winget\manifests\k\KrisnaSantosa15\notch\0.5.1
```

---

## Submitting to `microsoft/winget-pkgs`

You can submit or update the package using Microsoft's official [`wingetcreate`](https://github.com/microsoft/winget-create) tool:

1. Install `wingetcreate`:
   ```powershell
   winget install Microsoft.WingetCreate
   ```

2. Submit the latest release URL:
   ```powershell
   wingetcreate update KrisnaSantosa15.notch -v 0.5.1 -u https://github.com/KrisnaSantosa15/notch/releases/download/v0.5.1/notch-windows-x86_64.zip --submit
   ```
   Or submit from the validated manifest in this folder:
   ```powershell
   wingetcreate submit winget\manifests\k\KrisnaSantosa15\notch\0.5.1
   ```
