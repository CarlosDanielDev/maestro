---
name: security-patterns
version: "1.0.0"
description: Universal security patterns based on OWASP Top 10 and security best practices. Framework-agnostic.
allowed-tools: Read, Grep, Glob, WebSearch
---

# Security Patterns

## OWASP Top 10 (2021) Quick Reference

| # | Category | Key Check |
|---|----------|-----------|
| A01 | Broken Access Control | Auth checks on every endpoint |
| A02 | Cryptographic Failures | No hardcoded secrets |
| A03 | Injection | Parameterized queries only |
| A04 | Insecure Design | Rate limiting, input validation |
| A05 | Security Misconfiguration | No debug in production |
| A06 | Vulnerable Components | Audit dependencies |
| A07 | Authentication Failures | Brute force protection |
| A08 | Data Integrity | Code signing, safe deserialization |
| A09 | Logging Failures | No sensitive data in logs |
| A10 | SSRF | Validate all URLs |

## Universal Checklist

- [ ] No hardcoded secrets in source code
- [ ] No sensitive data in logs
- [ ] Input validation on all external boundaries
- [ ] Dependencies audited for CVEs
- [ ] Proper error handling (no stack traces exposed)
- [ ] Encryption for data at rest and in transit
