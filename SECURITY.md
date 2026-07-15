# Security Policy

## Supported versions

BudBuk is in early development. Security fixes are applied to the `main` branch and the
most recent release.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, report them privately via a
[GitHub security advisory](https://github.com/budbuk/budbuk/security/advisories/new).
We aim to acknowledge reports within a few business days and will keep you informed as we
investigate and remediate.

When reporting, please include:

- A description of the vulnerability and its impact
- Steps to reproduce (a minimal proof of concept if possible)
- Affected version(s) or commit
- Any suggested remediation

## Handling of credentials

BudBuk connectors require credentials for external services (e.g. Jira API tokens). To
keep them safe:

- **Never commit credentials.** `.env` is git-ignored; use it for local development and
  copy from `.env.example`.
- Credentials are read from environment variables at runtime and are **not** logged.
- If you believe a credential has been exposed (for example, pasted into an issue, PR, or
  log), **rotate it immediately** in the provider's console.
- Cache entries and rate-limit state are isolated per account so credentials for one
  account are never used for another.

## Scope

This policy covers the BudBuk source code in this repository. Vulnerabilities in
third-party dependencies should be reported upstream; we monitor advisories via
`cargo-deny` and Dependabot and will update affected dependencies promptly.
