# llmfuck

`llmfuck` provides LLM-generated corrections for the previous shell command. The installed command is `fuck`.

The project is under active development. Bash and Zsh are supported on Unix, and ordinary PowerShell 7 integration is included for Windows. Windows ConPTY capture is planned.

## Safety model

- Failed commands are never rerun to collect output.
- Ordinary mode sends no terminal output.
- Smart context is collected locally with bounded, command-specific collectors.
- Known secrets are redacted before a provider request.
- The model must classify every candidate's risk; a local classifier can only increase it.
- High-risk candidates require a second Enter press.
- Selected commands are still model output. Read every command before executing it.

## Build

```sh
cargo build --release
cargo install --path .
```

The project is licensed under the GNU Affero General Public License, version 3 or later. Third-party Rust dependencies retain their own licenses.

## Configure

```sh
fuck config
```

The wizard configures an OpenAI-compatible Chat Completions endpoint, stores the API key in the platform credential store, enables Smart privacy mode, and offers to install ordinary shell integration. It warns about provider disclosure before accepting any credential.

Manual commands:

```sh
fuck provider add local --endpoint http://127.0.0.1:11434/v1/chat/completions --model MODEL --no-api-key
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
