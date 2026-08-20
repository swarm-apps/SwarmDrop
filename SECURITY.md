# Security Policy

SwarmDrop moves files between devices with end-to-end encryption. If you find a way to
break that, we want to hear about it.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Use GitHub's private vulnerability reporting:
[**Report a vulnerability**](https://github.com/swarm-apps/SwarmDrop/security/advisories/new).
It is enabled on this repository and keeps the discussion private until a fix ships.

Useful things to include: affected version and platform, what an attacker needs in order
to pull it off (network position, an existing pairing, physical access …), and a
reproduction if you have one.

**On response times, honestly:** SwarmDrop is maintained by one person, not a company
with an on-call rotation. We aim to acknowledge a report within a few days and will keep
you posted on the fix. We won't promise a 24-hour SLA we can't keep. There is currently
no bug bounty.

## Supported versions

Only the **latest release of each line** receives fixes. There are no long-term support
branches — the project moves fast and old builds are superseded rather than patched.

| Line | Tag pattern | Supported |
|---|---|---|
| Desktop | `v*` | latest release only |
| Mobile | `mobile-v*` | latest release only |
| CLI | `cli/swarmdrop-cli-v*` | latest release only |
| Browser | continuously deployed | current deployment |

## What we consider a vulnerability

The security goal is narrow and testable: **nobody except the sender and the receiver can
read file contents** — not bootstrap nodes, not relays, not us. Anything that breaks the
following is in scope:

- Reading file contents without being the intended recipient.
- Impersonating a paired device, or completing a pairing you were not invited to.
- Reusing or forging a pair invite (they are Ed25519-signed, capability-bearing, and
  single-use with a 24h TTL).
- Defeating per-chunk BLAKE3 / bao-tree verification so corrupted or substituted data is
  accepted as authentic.
- Extracting the Ed25519 device private key from wherever the platform holds it — the OS
  secure store on mobile, an owner-only `identity.json` (`0600` on unix) on desktop and in
  the CLI.
- Making the MCP server reachable from outside `127.0.0.1`, or driving it to send files
  from a device whose MCP permission is off.
- Remote code execution, path traversal on received files, or anything that writes
  outside the chosen inbox directory.

## What is out of scope

These are deliberate design positions, not oversights:

- **Relays see connection metadata.** A relay forwards ciphertext it holds no key for,
  but it necessarily observes that two peers are talking, and roughly how much. Hiding
  metadata would require onion routing, which is not a goal of this project.
- **A compromised device is game over.** Files exist in plaintext on the sending and
  receiving machines by definition. Malware or an attacker with your unlocked session
  can read them; SwarmDrop cannot defend that layer.
- **Voluntarily shared invites.** An invite is a bearer capability. If you hand it to
  someone, they can pair. That is what it is for.
- **There is no application-layer encryption**, by design — confidentiality is the
  transport layer's job (Noise / QUIC-TLS). An extra XChaCha20-Poly1305 layer was
  removed in wire v2 because it was self-referential (its key travelled over the very
  same Noise channel) and could not coexist with per-chunk verification. "You could add
  another encryption layer" is not a vulnerability report.
- Anything requiring physical access to an unlocked device.
- Vulnerabilities in libp2p, Tauri, or other upstream dependencies — report those
  upstream, though we appreciate a heads-up so we can bump the pin.

## Security model

The full model — key hierarchy, transport encryption, zero-trust relaying, and integrity
verification — is documented at
[**swarm-apps.github.io/SwarmDrop/docs/security**](https://swarm-apps.github.io/SwarmDrop/docs/security).

Briefly: Ed25519 device identity — in the OS secure store on mobile, in an owner-only file
on desktop and in the CLI — Noise or TLS 1.3 on every connection with fresh ephemeral keys
and mutual authentication, BLAKE3 + bao-tree per-chunk verification, and relays that only
ever forward ciphertext.

## Disclosure

We prefer coordinated disclosure. Tell us first, give us a reasonable window to ship a
fix, and we will credit you in the release notes unless you would rather stay anonymous.
