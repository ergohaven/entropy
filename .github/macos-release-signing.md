# macOS stable permission identity prototype

Entropy currently ships GitHub artifact files without Apple Developer Program membership. Developer ID signing and notarization are therefore unavailable.

This prototype uses one project-owned self-signed code-signing certificate across releases. Its only goal is a stable, certificate-anchored designated requirement so macOS privacy controls can recognize changed binaries as the same app.

## Limits

- Apple explicitly advises against shipping self-signed apps.
- Gatekeeper does not trust this certificate. Users still see an unidentified-developer warning and may need Privacy & Security > Open Anyway.
- Apple notarization remains unavailable.
- TCC persistence must be verified manually on supported macOS versions before this workflow ships.
- Losing or rotating the private key changes app identity and requires users to grant Accessibility and Input Monitoring again.
- A compromised private key lets an attacker sign code that matches Entropy's permission identity. Keep it restricted to release maintainers.

Do not replace the certificate anchor with a bundle-ID-only requirement. Any binary can copy a bundle ID; Accessibility grants make that unsafe.

## Create release identity

Create this identity once on a secure Mac using Keychain Access > Certificate Assistant > Create a Certificate:

1. Name: `Entropy Open Source Release Signing`.
2. Identity Type: Self Signed Root.
3. Certificate Type: Code Signing.
4. Enable Let me override defaults and choose a long validity period.
5. Export certificate and private key together as an encrypted `.p12`.

The common name must remain exact because release workflow uses it to select identity. Store encrypted `.p12` and password in maintainer-controlled offline backup. Never commit private material.

Configure two GitHub Actions repository secrets:

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12_BASE64` | `base64 -i entropy-release-signing.p12` output |
| `MACOS_CERTIFICATE_PASSWORD` | `.p12` export password |

Release workflow imports identity into a temporary keychain, derives certificate hash, embeds an explicit requirement containing that certificate plus `com.ergohaven.entropy`, signs app, and rejects ad-hoc or mismatched output. DMG is not notarized. Shipped requirement does not contain `anchor trusted`; users do not install or trust this certificate.

## Automated proof

Run:

```bash
scripts/test_macos_stable_signing.sh
scripts/test_macos_stable_identity_e2e.sh
```

End-to-end test creates temporary self-signed identity and two different binaries. Test passes only when code hashes differ while designated requirements match. Temporary keychain and keys are deleted on exit. Run it on macOS 26 before manual TCC testing; macOS 15 runners do not expose generic OpenSSL-generated self-signed certificates as code-signing identities.

## Required manual TCC test

1. Produce two Entropy app builds with different binaries and same release identity.
2. Install first build as `/Applications/Entropy.app`.
3. Remove old Entropy entries from Accessibility and Input Monitoring. Add current app, grant both permissions, and verify Universal Symbols.
4. Replace app with second build without changing path.
5. Launch second build. Verify Universal Symbols still work without removing or re-adding permission entries.
6. Repeat on Apple Silicon and Intel machines.

Inspect both builds:

```bash
codesign -dv --verbose=4 Entropy.app
codesign -d -r- Entropy.app
spctl --assess --type execute --verbose=4 Entropy.app
```

Expected designated requirement:

```text
designated => certificate root = H"<same-certificate-sha1>" and identifier "com.ergohaven.entropy"
```

`spctl` rejection is expected for this prototype. Any `cdhash`-only requirement, changed certificate hash, or repeated TCC grant invalidates prototype.
