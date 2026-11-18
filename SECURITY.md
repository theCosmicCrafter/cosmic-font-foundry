# Security Policy

## Supported Versions

| Version | Supported |
| :--- | :--- |
| `0.3.x` | Yes |
| `< 0.3.0` | No — please upgrade to the latest release |

## Security Architecture & Guarantees

Cosmic Font Foundry is built to run locally on your machine with minimal attack surface:
- **Zero Telemetry / Zero Bundled ML:** No analytics, background phoning home, or bundled ML runtimes.
- **Essential Windows Font Guards:** Core Windows UI fonts (`Segoe UI`, `Arial`, `Consolas`, `Tahoma`, etc.) are permanently locked against deactivation or quarantine. System fonts require explicit user unlock and UAC elevation, with backups stored prior to modification.
- **Custom Feed SSRF Protection:** User-added RSS/Atom/JSON font feeds in Discover validate URLs via `is_public_url`, blocking `localhost`, loopback (`127.0.0.1`, `::1`), `.local`/`.internal` hostnames, and private IP ranges.
- **Loopback-Only MCP & REST Control Plane:** The optional Agent Hub control server binds strictly to `127.0.0.1` (ports `17420–17430`), enforces Read-Only mode when `allow_mutations` is disabled, and excludes stored BYO-AI API keys from unauthenticated HTTP status responses.

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Instead, report vulnerabilities privately through [GitHub Security Advisories](https://github.com/theCosmicCrafter/cosmic-font-foundry/security/advisories/new) with:
- A clear description of the vulnerability and affected component
- Steps to reproduce or a minimal proof-of-concept
- Potential impact assessment

You can expect an acknowledgment within 72 hours and regular updates until a fix is released.
