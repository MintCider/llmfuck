# llmfuck

`llmfuck` provides LLM-generated corrections for the previous shell command. The installed command is `fuck`.

The project is under active development. Bash and Zsh are supported on Unix, and ordinary PowerShell 7 integration is included for Windows. Windows ConPTY capture is planned.

## Install v0.0.1 prerelease

Release binaries are available for Linux x86_64, macOS Intel and Apple Silicon, and Windows x86_64. Download `SHA256SUMS` from the same [GitHub release](https://github.com/MintCider/llmfuck/releases/tag/v0.0.1) and verify the archive before installing it.

Linux x86_64:

```sh
version=v0.0.1
target=x86_64-unknown-linux-gnu
archive="llmfuck-$version-$target.tar.gz"
curl -fLO "https://github.com/MintCider/llmfuck/releases/download/$version/$archive"
curl -fLO "https://github.com/MintCider/llmfuck/releases/download/$version/SHA256SUMS"
grep " $archive$" SHA256SUMS | sha256sum --check
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "llmfuck-$version-$target/fuck" "$HOME/.local/bin/fuck"
```

macOS:

```sh
version=v0.0.1
case "$(uname -m)" in
  arm64) target=aarch64-apple-darwin ;;
  x86_64) target=x86_64-apple-darwin ;;
  *) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac
archive="llmfuck-$version-$target.tar.gz"
curl -fLO "https://github.com/MintCider/llmfuck/releases/download/$version/$archive"
curl -fLO "https://github.com/MintCider/llmfuck/releases/download/$version/SHA256SUMS"
grep " $archive$" SHA256SUMS | shasum -a 256 --check
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "llmfuck-$version-$target/fuck" "$HOME/.local/bin/fuck"
```

Windows x86_64, from PowerShell 7:

```powershell
$Version = 'v0.0.1'
$Target = 'x86_64-pc-windows-msvc'
$Archive = "llmfuck-$Version-$Target.zip"
$BaseUrl = "https://github.com/MintCider/llmfuck/releases/download/$Version"
Invoke-WebRequest "$BaseUrl/$Archive" -OutFile $Archive
Invoke-WebRequest "$BaseUrl/SHA256SUMS" -OutFile SHA256SUMS
$Expected = ((Get-Content SHA256SUMS | Where-Object { $_ -match " $([regex]::Escape($Archive))$" }) -split '\s+')[0]
if ((Get-FileHash $Archive -Algorithm SHA256).Hash -ne $Expected) { throw 'SHA-256 verification failed' }
Expand-Archive $Archive -DestinationPath . -Force
$BinDir = Join-Path $HOME '.local\bin'
New-Item -ItemType Directory -Force $BinDir | Out-Null
Copy-Item "llmfuck-$Version-$Target\fuck.exe" $BinDir
$UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($UserPath -split ';') -notcontains $BinDir) {
  [Environment]::SetEnvironmentVariable('Path', "$UserPath;$BinDir", 'User')
}
```

Open a new shell, confirm the binary, then configure a provider and ordinary shell integration:

```sh
fuck --version
fuck config
fuck doctor
```

The configuration wizard can install ordinary Bash, Zsh, or PowerShell integration. You can also run `fuck init bash`, `fuck init zsh`, or `fuck init pwsh` explicitly, then open a new shell.

If `$HOME/.local/bin` is not already on `PATH`, move the binary to a directory that is on `PATH` or add that directory using your operating system's normal environment configuration.

## Safety model

- Failed commands are never rerun to collect output.
- Ordinary mode sends no terminal output.
- Smart context is collected locally with bounded, command-specific collectors.
- Known secrets are redacted before a provider request.
- The model must classify every candidate's risk; a local classifier can only increase it.
- High-risk candidates require a second Enter press.
- Selected commands are still model output. Read every command before executing it.

## Build from source

```sh
cargo build --release
cargo install --path .
```

The project is licensed under the GNU Affero General Public License, version 3 or later. Third-party Rust dependencies retain their own licenses.

## Configure

```sh
fuck config
```

The wizard configures an OpenAI-compatible Chat Completions endpoint, tries to store the API key in the platform credential store, enables Smart privacy mode, and offers to install ordinary shell integration. If secure storage is unavailable, it explains the error and asks whether to store the key unencrypted in `config.toml` instead. Plaintext storage is never selected without confirmation. On Unix, the configuration directory and file are restricted to modes `0700` and `0600`.

Manual commands:

```sh
fuck provider add local --endpoint http://127.0.0.1:11434/v1/chat/completions --model MODEL --no-api-key
fuck provider add hosted --endpoint https://example.com/v1/chat/completions --model MODEL --plaintext-api-key
fuck provider list
fuck provider use local
fuck privacy set minimal
fuck privacy set smart
fuck context --command 'git chekout main' --shell zsh --exit-code 1
fuck status
fuck doctor
```

## Shell integration

```sh
fuck init bash
fuck init zsh
fuck init pwsh
fuck init --reverse
```

`fuck init` updates a marked block in the selected user profile and writes a backup beside it. It does not enable PTY capture.

After a failed command, run:

```console
$ git chekout main
$ fuck
```

Use Up/Down or `j`/`k` to select, Right or `l` to expand the selected effect, Enter to execute, and Esc to cancel.

## PTY capture

Run `fuck pty` for manual setup guidance. The configuration wizard only mentions this mode and never starts it or edits a terminal profile. See [docs/pty.md](docs/pty.md) for limitations and privacy details.

## Provider request

The first release uses `POST /v1/chat/completions` semantics and basic `system`/`user` messages. It does not depend on tool calling or vendor-specific structured-output features.
