---
name: security-patterns
version: "1.0.0"
description: Universal security patterns based on OWASP Top 10 and security best practices. Framework-agnostic security guidance.
allowed-tools: Read, Grep, Glob, WebSearch
---

# Security Patterns

Universal security patterns and OWASP Top 10 compliance. Framework-agnostic security guidance.

## Skill Usage

| Aspect | Details |
|--------|---------|
| **Consumer** | `subagent-security-analyst` (and all architects) |
| **Purpose** | Security vulnerability detection and remediation patterns |
| **Invocation** | Subagents read this skill; NOT directly invocable by users |
| **Applicability** | All frameworks and languages |

---

## OWASP Top 10 (2021) - Quick Reference

| # | Vulnerability | Description | Prevention |
|---|---------------|-------------|------------|
| A01 | **Broken Access Control** | Missing or improper authorization | Implement proper access controls, deny by default |
| A02 | **Cryptographic Failures** | Weak encryption, exposed secrets | Use strong encryption, protect data at rest/transit |
| A03 | **Injection** | SQL, NoSQL, Command injection | Use parameterized queries, ORM, input validation |
| A04 | **Insecure Design** | Missing security requirements | Apply threat modeling, secure design patterns |
| A05 | **Security Misconfiguration** | Default configs, verbose errors | Harden configs, disable debug in production |
| A06 | **Vulnerable Components** | Outdated dependencies | Keep dependencies updated, scan for CVEs |
| A07 | **Auth/Session Issues** | Weak authentication | Use MFA, secure session management, strong passwords |
| A08 | **Data Integrity Failures** | Unsigned/unverified data | Implement digital signatures, integrity checks |
| A09 | **Logging Failures** | Insufficient logging | Log security events, monitor anomalies |
| A10 | **SSRF** | Server-side request forgery | Validate URLs, use allowlists, network segmentation |

---

## Common Security Patterns

### Input Validation

```
ALWAYS validate and sanitize user input:
1. Validate type, length, format, range
2. Use allowlist (not denylist) when possible
3. Sanitize before processing or storing
4. Encode before rendering (prevent XSS)
```

### Authentication

```
Best practices:
1. Never store passwords in plaintext
2. Use bcrypt, scrypt, or Argon2 for hashing
3. Implement account lockout after failed attempts
4. Use MFA where possible
5. Secure password reset flows
```

### Authorization

```
Implement principle of least privilege:
1. Check permissions on EVERY request
2. Use role-based access control (RBAC)
3. Deny by default
4. Validate on server-side (never trust client)
```

### SQL Injection Prevention

```
1. Use parameterized queries/prepared statements
2. Use ORMs with proper escaping
3. Never concatenate user input into queries
4. Apply principle of least privilege to DB users
```

### XSS Prevention

```
1. Escape output based on context (HTML, JS, CSS, URL)
2. Use Content Security Policy (CSP) headers
3. Sanitize HTML if allowing user HTML
4. Use frameworks that auto-escape by default
```

### CSRF Prevention

```
1. Use anti-CSRF tokens
2. Validate token on state-changing requests
3. Use SameSite cookie attribute
4. Verify Origin/Referer headers
```

---

## Detailed Guides

For complete implementation details, read:

- **[owasp-top-10.md](owasp-top-10.md)** - Detailed OWASP Top 10 prevention
- **[auth-patterns.md](auth-patterns.md)** - Authentication best practices
- **[api-security.md](api-security.md)** - API security patterns
- **[crypto-patterns.md](crypto-patterns.md)** - Cryptography best practices

---

## Security Checklist

### Authentication & Authorization
- [ ] Passwords hashed with strong algorithm (bcrypt, Argon2)
- [ ] MFA implemented for sensitive operations
- [ ] Session tokens are cryptographically random
- [ ] Session timeout implemented
- [ ] Authorization checked on every request
- [ ] Principle of least privilege applied

### Input Validation
- [ ] All user input validated server-side
- [ ] Allowlist validation used where possible
- [ ] File uploads restricted by type and size
- [ ] SQL injection prevented (parameterized queries)
- [ ] XSS prevented (output encoding)
- [ ] Command injection prevented

### Data Protection
- [ ] Sensitive data encrypted at rest
- [ ] TLS/HTTPS enforced for data in transit
- [ ] Secrets not hardcoded in code
- [ ] Environment variables used for secrets
- [ ] Database credentials properly secured

### API Security
- [ ] Rate limiting implemented
- [ ] CORS configured properly
- [ ] API keys secured and rotated
- [ ] Input validation on all endpoints
- [ ] Error messages don't expose internal details

### Headers & Configuration
- [ ] Security headers configured (CSP, X-Frame-Options, etc.)
- [ ] Debug mode disabled in production
- [ ] Unnecessary services/ports disabled
- [ ] Default passwords changed
- [ ] Error pages don't expose stack traces

### Logging & Monitoring
- [ ] Authentication events logged
- [ ] Authorization failures logged
- [ ] Security events monitored
- [ ] Logs don't contain sensitive data
- [ ] Log integrity protected

---

## Security Headers (Universal)

```
Content-Security-Policy: default-src 'self'
X-Frame-Options: DENY
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
Permissions-Policy: geolocation=(), microphone=()
Strict-Transport-Security: max-age=31536000; includeSubDomains
```

---

## Common Vulnerabilities to Flag

### Critical
- ❌ SQL injection possible (string concatenation in queries)
- ❌ Passwords stored in plaintext
- ❌ Hardcoded secrets/API keys in code
- ❌ No authentication on sensitive endpoints
- ❌ Command injection possible

### High
- ❌ Weak password hashing (MD5, SHA1)
- ❌ Missing authorization checks
- ❌ XSS vulnerabilities
- ❌ CSRF vulnerabilities
- ❌ Insecure deserialization

### Medium
- ❌ Missing rate limiting
- ❌ Verbose error messages in production
- ❌ Missing security headers
- ❌ Session tokens not secure
- ❌ Insufficient logging

---

## Framework-Specific Security

### Node.js/Express
```javascript
// Helmet for security headers
const helmet = require('helmet')
app.use(helmet())

// Rate limiting
const rateLimit = require('express-rate-limit')
app.use(rateLimit({ windowMs: 15 * 60 * 1000, max: 100 }))

// Parameterized queries
db.query('SELECT * FROM users WHERE id = $1', [userId])
```

### Python/Django
```python
# Django has many security features built-in:
# - CSRF protection (middleware)
# - XSS protection (template auto-escaping)
# - SQL injection protection (ORM)

# Ensure these settings:
SECURE_SSL_REDIRECT = True
SESSION_COOKIE_SECURE = True
CSRF_COOKIE_SECURE = True
```

### Java/Spring Boot
```java
// Spring Security configuration
@Configuration
@EnableWebSecurity
public class SecurityConfig extends WebSecurityConfigurerAdapter {
    @Override
    protected void configure(HttpSecurity http) throws Exception {
        http
            .csrf().and()
            .headers()
                .contentSecurityPolicy("default-src 'self'");
    }
}
```

---

## Tools for Security Testing

- **SAST**: SonarQube, Checkmarx, Semgrep
- **DAST**: OWASP ZAP, Burp Suite
- **Dependency Scanning**: Snyk, Dependabot, npm audit
- **Secret Scanning**: GitLeaks, TruffleHog

---

## When to Consult This Skill

- Reviewing code for security vulnerabilities
- Implementing authentication/authorization
- Designing API security
- Adding input validation
- Configuring security headers
- Preventing OWASP Top 10 vulnerabilities
- Security testing strategies
