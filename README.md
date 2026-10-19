# MLE

A lightweight native Windows selection translator written in Rust. Local
English-Chinese dictionary first; optional Azure Translator for missing words
and sentences. No browser engine, local language model, clipboard modifications,
or selection history.

## Run

Windows 10/11 x64. Run `dist\MLE\mle.exe` after packaging. Keep the `data` and
`licenses` folders beside the executable. The first launch opens Settings;
subsequent launches stay in the notification area. Click the tray icon to open
Settings, or right-click it to exit.

- Select text in another application and hold the right mouse button for 500 ms.
- Alternatively select **Custom keyboard shortcut** in Settings and press the
  desired combination in the shortcut box (default: Ctrl+Alt+T).
- Ordinary short right-clicks are replayed. Right drags start after moving
  8 pixels. This interception is not identical to native input in every app;
  keyboard mode is recommended for games and unusual mouse interactions.
- Results appear without taking focus. Esc, an outside click, or changing the
  foreground application dismisses the popup. Long results can be scrolled.

Selection capture uses UI Automation. Compatibility depends on the source
application's accessibility implementation. Image text, password fields, secure
desktop, and higher-privilege applications are not supported. No clipboard
fallback or OCR is performed.

## Settings and privacy

Local dictionary lookup needs no account or internet. The bundled dictionary is
under 10,000,000 bytes, includes only English headwords and Chinese meanings, and
is queried through a read-only SQLite connection with a 256 KiB page-cache
budget. It is not loaded wholesale. Inflection tables and pronunciation are
not included; words absent from the dictionary can use online translation.

To enable online translation, create an Azure Translator resource, enter its
**subscription key** and region (for example, `westus2`), and explicitly check
the online translation consent box. This is an Azure API key, not an arbitrary
provider's token. Leave the key field blank to retain a previously saved key.
Use **Delete saved key** to remove it.

Only explicitly triggered selections missing from the dictionary are sent to
Microsoft Azure. Requests use the documented Translator v3 REST endpoint,
automatic source-language detection, and Simplified Chinese (`zh-Hans`).
Azure usage limits and charges apply. The application uses the system WinHTTP
proxy/TLS implementation and does not bypass certificate verification.

Keys are stored as per-user Windows generic credentials under
`MLE/AzureTranslator`, not in source or settings. Non-secret preferences live
in `%APPDATA%\MLE\settings.json`. Windows credentials are not protection against
other malicious programs running as the same user.

## Build

Requires stable Rust and the MSVC C++ build tools / Windows SDK.

```powershell
cargo check
.\tools\package.ps1
```

The generated dictionary is included. To regenerate it, use Python 3:

```powershell
python tools\build_dictionary.py
```

The builder pins the ECDICT source revision and verifies the Git blob checksum.
It prioritizes common words, retains only headwords/Chinese meanings, and
measures the actual indexed SQLite file against the 10 MB budget.
Upstream licensing and attribution: `licenses\ECDICT.txt`.

For a direct dictionary lookup without desktop integration:

```powershell
.\dist\MLE\mle.exe --lookup hello lookup-result.txt
```

## Platform boundary

`dictionary`, `translation`, and configuration data are independent of native
UI APIs. `windows_app` owns hooks, UI Automation, native windows, credential
storage, and WinHTTP. A future macOS adapter needs AppKit/Accessibility,
Keychain, and a native HTTP transport. **The macOS desktop adapter is not
implemented in this version.**
