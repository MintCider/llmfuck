# llmfuck

`llmfuck` provides LLM-generated corrections for the previous shell command and generates commands from an explicit intent. The installed command is `fuck`.

The project is under active development. Bash and Zsh are supported on Unix, and ordinary PowerShell 7 integration is included for Windows. Windows ConPTY capture is planned.

## Install v0.0.3 prerelease

Release binaries are available for Linux x86_64, macOS Intel and Apple Silicon, and Windows x86_64. The release installers download into a temporary directory, verify the archive against `SHA256SUMS`, and install only the `fuck` binary into the user-local bin directory.

Linux or macOS:

```sh
curl -fsSL https://github.com/MintCider/llmfuck/releases/download/v0.0.3/install.sh | sh
```

Windows x86_64, from PowerShell 7:

```powershell
irm https://github.com/MintCider/llmfuck/releases/download/v0.0.3/install.ps1 | iex
```

These commands execute a downloaded script. Review [`install.sh`](scripts/install.sh) or [`install.ps1`](scripts/install.ps1) first if that does not match your security policy. Manual installation from the [v0.0.3 GitHub release](https://github.com/MintCider/llmfuck/releases/tag/v0.0.3) is documented below.

Linux x86_64:

```sh
(
  set -eu
  version=v0.0.3
  target=x86_64-unknown-linux-gnu
  archive="llmfuck-$version-$target.tar.gz"
  base_url="https://github.com/MintCider/llmfuck/releases/download/$version"
  temp_dir=$(mktemp -d)
  trap 'rm -rf -- "$temp_dir"' EXIT
  curl -fL "$base_url/$archive" -o "$temp_dir/$archive"
  curl -fL "$base_url/SHA256SUMS" -o "$temp_dir/SHA256SUMS"
  checksum_line=$(grep " $archive$" "$temp_dir/SHA256SUMS")
  (cd "$temp_dir" && printf '%s\n' "$checksum_line" | sha256sum --check)
  tar -xzf "$temp_dir/$archive" -C "$temp_dir"
  mkdir -p "$HOME/.local/bin"
  install -m 755 "$temp_dir/llmfuck-$version-$target/fuck" "$HOME/.local/bin/fuck"
)
```

macOS:

```sh
(
  set -eu
  version=v0.0.3
  case "$(uname -m)" in
    arm64) target=aarch64-apple-darwin ;;
    x86_64) target=x86_64-apple-darwin ;;
    *) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
  esac
  archive="llmfuck-$version-$target.tar.gz"
  base_url="https://github.com/MintCider/llmfuck/releases/download/$version"
  temp_dir=$(mktemp -d)
  trap 'rm -rf -- "$temp_dir"' EXIT
  curl -fL "$base_url/$archive" -o "$temp_dir/$archive"
  curl -fL "$base_url/SHA256SUMS" -o "$temp_dir/SHA256SUMS"
  checksum_line=$(grep " $archive$" "$temp_dir/SHA256SUMS")
  (cd "$temp_dir" && printf '%s\n' "$checksum_line" | shasum -a 256 --check)
  tar -xzf "$temp_dir/$archive" -C "$temp_dir"
  mkdir -p "$HOME/.local/bin"
  install -m 755 "$temp_dir/llmfuck-$version-$target/fuck" "$HOME/.local/bin/fuck"
)
```

Windows x86_64, from PowerShell 7:

```powershell
$Version = 'v0.0.3'
$Target = 'x86_64-pc-windows-msvc'
$Archive = "llmfuck-$Version-$Target.zip"
$BaseUrl = "https://github.com/MintCider/llmfuck/releases/download/$Version"
$TempDir = Join-Path ([IO.Path]::GetTempPath()) "llmfuck-$([guid]::NewGuid())"
New-Item -ItemType Directory $TempDir | Out-Null
try {
  $ArchivePath = Join-Path $TempDir $Archive
  $ChecksumsPath = Join-Path $TempDir 'SHA256SUMS'
  Invoke-WebRequest "$BaseUrl/$Archive" -OutFile $ArchivePath
  Invoke-WebRequest "$BaseUrl/SHA256SUMS" -OutFile $ChecksumsPath
  $Expected = ((Get-Content $ChecksumsPath | Where-Object { $_ -match " $([regex]::Escape($Archive))$" }) -split '\s+')[0]
  if ((Get-FileHash $ArchivePath -Algorithm SHA256).Hash -ne $Expected) { throw 'SHA-256 verification failed' }
  Expand-Archive $ArchivePath -DestinationPath $TempDir -Force
  $BinDir = Join-Path $HOME '.local\bin'
  New-Item -ItemType Directory -Force $BinDir | Out-Null
  Copy-Item (Join-Path $TempDir "llmfuck-$Version-$Target\fuck.exe") $BinDir
  $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (($UserPath -split ';') -notcontains $BinDir) {
    [Environment]::SetEnvironmentVariable('Path', "$UserPath;$BinDir", 'User')
  }
} finally {
  Remove-Item -Recurse -Force $TempDir
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
- Every `git push` is treated as high risk, regardless of the model's classification.
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
fuck provider set hosted --reasoning-effort low
fuck provider set hosted --clear-reasoning-effort
fuck provider list
fuck provider use local
fuck provider latency
fuck provider latency hosted local --runs 3
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

Or describe the command you want:

```console
$ fuck I want to pull the remote branch on upstream/master
```

An explicit prompt does not include the previous command, its exit code, or its terminal output in the provider request. Smart mode may still add the documented read-only environment context.

Use Up/Down or `j`/`k` to select, Right or `l` to expand the selected effect, Enter to execute, and Esc to cancel. The selected command is printed with a `Running:` prefix before execution so that silent or slow commands remain identifiable.

## PTY capture

Run `fuck pty` for manual setup guidance. The configuration wizard only mentions this mode and never starts it or edits a terminal profile. See [docs/pty.md](docs/pty.md) for limitations and privacy details.

## Provider request

The first release uses `POST /v1/chat/completions` semantics and basic `system`/`user` messages. It does not depend on tool calling or vendor-specific structured-output features.

`fuck provider latency` sends the fixed intent `Print the current working directory.` to configured providers in parallel and measures the time until usable candidates are returned. The probe uses a synthetic cwd and does not include command history, terminal output, Git context, or the real working directory. One request per provider is sent by default; use `--runs` for repeated measurements.
