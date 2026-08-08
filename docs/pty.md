# PTY capture mode

PTY capture is optional and must be enabled by manually changing a terminal profile. The ordinary Bash or Zsh integration must remain installed because it emits command-boundary metadata.

Example terminal profile commands:

```sh
fuck shell -- zsh -l
fuck shell -- bash -l
```

The configuration wizard never makes this change.

## What it records

The proxy forwards terminal bytes and retains at most 128 KiB for the current command and five completed records in process memory. Records are discarded when the proxy exits and are not written to disk. The `fuck` process requests the previous record over a user-only Unix socket.

PTY output combines stdout and stderr. It can contain prompts, command echo, control sequences, passwords, source code, filenames, and business data. Smart mode strips common terminal control sequences, applies a size limit, and redacts known secret patterns before sending context. Minimal mode never includes terminal output.

Secret detection is not complete. Do not enable PTY capture where local in-memory recording is unacceptable.

## Transparency limitations

The child shell receives a PTY, so interactive applications generally continue to detect a terminal. The mode is still observable:

- `tty` reports the nested PTY.
- The process tree contains a `fuck shell` proxy.
- stdout and stderr are merged.
- Terminal resizing and uncommon terminal protocols may not behave identically in this early implementation.
- Nested tmux, screen, SSH, and full-screen applications require additional testing.

Windows ConPTY capture is not implemented yet. PowerShell 7 ordinary mode remains available on Windows.
