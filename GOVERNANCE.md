# Project Governance

## Overview

**Cosmic Font Foundry** is an open-source, MIT-licensed desktop font manager originally forked from [`atunfrito/tuna-fonts`](https://github.com/atunfrito/tuna-fonts) (MIT).

## Roles

- **Contributors:** Anyone who submits issues, bug reports, feature proposals, documentation improvements, or pull requests.
- **Maintainers:** Contributors trusted with repository merge access, release signing, dependency audits, and Code of Conduct enforcement (listed in [MAINTAINERS.md](MAINTAINERS.md)).

## Decision Making

- Routine bug fixes, documentation, and UI polish require one maintainer review and passing CI checks.
- Architectural changes (new font sources, Windows system/registry behavior, MCP tool additions, or licensing changes) are discussed in a GitHub Issue or Pull Request before merging to `main`.
- **Core Project Principles:**
  1. Stay lightweight and fast (~10 MB native footprint, zero bundled ML models).
  2. Never silently delete or corrupt user/system fonts—always prefer non-destructive toggling and quarantine.
  3. Preserve full local-first privacy and transparent MIT licensing.
