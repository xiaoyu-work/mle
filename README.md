# MLE - Make Life Easier

A lightweight native Windows selection translator written in Rust. Local
English-Chinese dictionary first; optional Azure OpenAI, Codex (ChatGPT), or
GitHub Copilot for missing words and sentences. No browser engine, local language
model, clipboard-based capture, or MLE selection history.

## Run

Windows 10/11 x64. Run `dist\MLE\mle.exe` after packaging. Keep the `data` and
`licenses` folders beside the executable. The first launch opens Settings;
subsequent launches stay in the notification area. Click the tray icon to open
Settings, or right-click it to exit.

- Select text in another application and hold the right mouse button for 500 ms.
  This timing is managed by the app, not a user setting.
- Alternatively select **Keyboard shortcut** in Settings and press the
  desired combination in the shortcut box (default: Ctrl+Alt+T).
- Ordinary short right-clicks are replayed. Right drags start after moving
  8 pixels. This interception is not identical to native input in every app;
  keyboard mode is recommended for games and unusual mouse interactions.
- Results appear without taking focus. Esc, an outside click, or changing the
  foreground application dismisses the popup. Long results can be scrolled.
- The compact popup shows only the result, without branding, headings, or original
  text. Dictionary meanings remain separated and long results can be scrolled.

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

The compact, single-surface Settings page uses inline fields and a font-size
slider rather than nested cards. **Translation font size** adjusts the result
text from 12 to 32 logical pixels.
The sample underneath updates immediately using the same font as the popup.
Save applies the size to future results; Cancel discards the change.

Choose **Azure OpenAI** to enter your Azure OpenAI deployment's full
**chat completions endpoint** and its **API key**, and save. For example:

```text
https://YOUR-RESOURCE.cognitiveservices.azure.com/openai/deployments/gpt-4.1-mini/chat/completions?api-version=2025-01-01-preview
```

Use the deployment name configured in Azure, not just the underlying model name.
Online translation is available automatically whenever a key is saved; there
is no separate switch.
No region setting or Python SDK is required. Use the API key for that Azure
OpenAI resource, not a Translator key or an endpoint URL. A saved key is shown
masked; **Show** reveals it only while Settings is open. Clear the field and save
to remove it. **Cancel** discards edits. Without a key, local lookup
still works.

Only explicitly triggered selections missing from the dictionary are sent to
your configured Azure resource. Requests use Chat Completions with a
translation-only system prompt, temperature zero, and Simplified Chinese output.
Filtered, refused, or truncated results are reported as errors rather than
shown as completed translations. Azure model token charges and usage limits
apply. The application uses the system WinHTTP proxy/TLS implementation and
does not bypass certificate verification.

Alternatively choose **Codex** or **Copilot**, then **Install** if the official
component is missing, and **Sign in**. Installation uses Windows Package Manager
(`OpenAI.Codex` / `GitHub.Copilot`); installation and login open an official
installation console; sign-in opens your default browser directly.
Keep MLE running until authorization completes: its official component receives
the local OAuth callback. Codex's hosted/streamlined success-page redirects are
disabled so signing in does not launch the Codex desktop app. MLE receives the
completion event and restores its own Settings window; Windows may require a
click on its taskbar notification to bring it to the foreground. After installation, click
**Sign in** again. **Refresh**
checks the current login. Save selects the translation source; switching sources
preserves your Azure endpoint and key. Already signed-in official CLI accounts
are reused. Codex requires ChatGPT sign-in, not an API key.
Codex uses the official component's recommended model rather than inheriting a
possibly incompatible model from the user's coding configuration.

These modes use your account's plan, policies, model availability, and usage
limits; they are not unlimited or a substitute for a general-purpose API key.
Quota, login and component errors are shown without silently falling back to
another source. Credentials and token refresh belong to the official component;
MLE does not copy OAuth tokens into its settings. The CLI's own credential-store
policy applies, including any file-based fallback.

The optional components run out of process only for account checks or online
translation, then exit. They are not bundled into the lightweight MLE executable
and have additional installation, startup, and peak-memory costs. Local
dictionary lookups and Azure requests never start them. Translation uses a
separate empty working directory, disables tool access and custom configuration
discovery where supported, and rejects approval requests. Codex threads are
ephemeral; Copilot sessions are destroyed after a response. Official components
can still maintain their own operational logs, caches, and account metadata.
The selected online provider receives the selected text; its privacy policy
applies.

Keys are stored as per-user Windows generic credentials under
`MLE/AzureOpenAI`, not in source or settings. Previously saved Translator keys
are not reused for this different service. Non-secret preferences live
in `%APPDATA%\MLE\settings.json`. Windows credentials are not protection against
other malicious programs running as the same user.

## Build

Requires stable Rust and the MSVC C++ build tools / Windows SDK.

```powershell
cargo check
.\tools\package.ps1
```

The executable, tray, and windows use the original icon in `assets\mle.ico`.
Its vector source is `assets\mle.svg`; regenerate the multi-resolution icon with
`.\tools\build_icon.ps1` (Windows drawing APIs, no extra image tools).

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
