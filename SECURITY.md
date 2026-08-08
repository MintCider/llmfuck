# Security

## Reporting

Do not include real credentials, private commands, or terminal output in a public issue. Use GitHub's private vulnerability reporting feature once the repository enables it.

## Trust boundaries

Command history, terminal output, filenames, project metadata, provider responses, and candidate descriptions are untrusted data.

Provider requests are minimized and redacted locally, but redaction cannot guarantee removal of every secret. Use Minimal privacy mode when command text alone is acceptable, inspect requests with `fuck context`, and review the retention policy of the configured provider.

Model risk classifications are advisory. A local classifier upgrades known hazardous patterns, but no classifier can identify every destructive command. High-risk commands require a second confirmation; all other selected commands execute after one Enter press.
