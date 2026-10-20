# Building MLE

Requires Windows 10/11 x64, stable Rust, and the MSVC C++ build tools / Windows SDK.
Run all commands from the repository root.

## Build and package

```powershell
cargo check
.\tools\package.ps1
```

The portable application is generated in `dist\MLE`. Keep the `data` and
`licenses` folders beside `mle.exe`.

## Manual GitHub Actions build

In the repository's **Actions** tab, select **Build Windows app**, then
**Run workflow**. It does not run automatically on pushes or pull requests.

Download the `MLE-windows-x64` artifact from the completed run and extract
`MLE-windows-x64.zip`. Open `MLE\mle.exe`, keeping the included folders beside it.
This is an unsigned portable package, not an installer; Windows may show a
SmartScreen warning.

## Icon

The icon is `assets\mle.ico`, with vector source in `assets\mle.svg`.
Regenerate the multi-resolution icon using Windows drawing APIs:

```powershell
.\tools\build_icon.ps1
```

## Dictionary

The generated dictionary is included. To regenerate it, use Python 3:

```powershell
python tools\build_dictionary.py
```

The builder pins the ECDICT source revision and verifies the Git blob checksum.
It prioritizes common words, retains only headwords and Chinese meanings, and
keeps the indexed SQLite file under 10 MB.
Upstream licensing and attribution: `licenses\ECDICT.txt`.

For a direct dictionary lookup without desktop integration:

```powershell
.\dist\MLE\mle.exe --lookup hello lookup-result.txt
```
