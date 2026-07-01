# Privacy Policy

Last updated: July 1, 2026

## Overview

Caretta collects **anonymous usage telemetry** to help improve the product and protect intellectual property. This policy explains what data we collect, why we collect it, and how you can opt out.

## Data Collection

### What We Collect

Caretta collects the following **anonymous** data through the [g-telemetry](https://github.com/geoffsee/g-telemetry) integration:

| Data Type | Description | Purpose |
|---|---|---|
| **App Metadata** | Version, platform (OS) | Understand usage across different environments |
| **Instance ID** | Random UUID v4 (persisted locally) | Distinguish unique installations anonymously |
| **Event Names** | App start, exit, command execution, workflow events | Track feature usage patterns |
| **Event Properties** | Command names, success/failure, duration | Measure reliability and performance |
| **Agent Info** | Agent type, model, action | Understand which agents are used |
| **Error Data** | Error type and message (no stack traces or content) | Identify and fix issues |
| **Timestamps** | When events occur | Analyze usage patterns over time |

### What We Do NOT Collect

We **explicitly do not** collect:

- Personally Identifiable Information (PII): names, email addresses, usernames, etc.
- IP addresses or network information
- File paths, directory structures, or repository contents
- User input, prompts, or conversation content
- Environment variables or configuration values
- System hostnames or MAC addresses
- Any data that could identify you or your organization

### Data Storage

- **Local**: Instance ID is stored at `~/.config/anon-telemetry/caretta/instance_id` on your machine
- **Remote**: Events are sent to `https://anon-telemetry-sink.seemueller.workers.dev/v1/events` (Cloudflare Workers)
- **Retention**: Raw events are processed and aggregated; we do not store individual event data long-term

## IP Protection

The telemetry endpoint URL and application ID are **hardcoded** in the source code and **cannot be overridden** by users. This design choice:

1. **Prevents redirect attacks**: Users cannot configure telemetry to send data to their own servers
2. **Protects our IP**: Ensures usage analytics flow only to our controlled infrastructure
3. **Maintains privacy**: The hardcoded endpoint is still bound by this privacy policy

## Opt-Out

You can disable telemetry at any time using any of these methods:

### Method 1: Environment Variable (Global)

```sh
# Disable all telemetry for any application respecting DNT
export DO_NOT_TRACK=1
caretta
```

### Method 2: App-Specific Environment Variable

```sh
# Disable only for Caretta
export CARETTA_NO_TELEMETRY=1
caretta
```

### Method 3: Configuration File

Add to your `caretta.toml`:

```toml
[telemetry]
enabled = false
```

### Verification

When telemetry is disabled, **no events are sent** and no instance ID is generated or stored. The telemetry client is not initialized.

## Data Usage

Collected data is used for:

- **Product improvement**: Identify most/least used features, common errors, performance bottlenecks
- **IP protection**: Understand legitimate usage patterns to detect unauthorized use
- **Bug fixing**: Prioritize fixes based on error frequency and impact
- **Roadmap planning**: Guide development priorities based on actual usage

We **do not** use this data for:

- Advertising or marketing
- Selling to third parties
- Building user profiles
- Tracking individual users across applications

## Data Security

- All telemetry is transmitted via **HTTPS** (TLS 1.3)
- The endpoint is hosted on **Cloudflare Workers** with enterprise-grade security
- No authentication is required because no sensitive data is transmitted
- Instance IDs are cryptographically random (UUID v4) and cannot be reversed to identify users

## Compliance

This telemetry collection is designed to be compliant with:

- **GDPR**: No PII is collected, and EU users have the right to opt-out
- **CCPA**: No personal data is sold or shared; users can opt-out
- **General privacy principles**: Data minimization, purpose limitation, user control

## Changes to This Policy

We may update this privacy policy from time to time. We will notify users of any changes by updating the "Last updated" date at the top of this document. Continued use of Caretta after changes are posted constitutes acceptance of those changes.

## Contact

If you have questions about this privacy policy or our data practices, please open an issue on the [Caretta GitHub repository](https://github.com/geoffsee/caretta).
