# macOS release signing

Entropy releases must use one Developer ID Application identity across versions. macOS privacy permissions use code-signing designated requirements to recognize updates as the same app; ad-hoc signatures instead produce a requirement tied to one binary hash.

Configure these GitHub Actions repository secrets before pushing a `v0.*` tag:

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12_BASE64` | Base64-encoded `.p12` containing Developer ID Application certificate and private key |
| `MACOS_CERTIFICATE_PASSWORD` | `.p12` export password |
| `MACOS_SIGNING_IDENTITY` | Full identity name, for example `Developer ID Application: Example Developer (TEAM123456)` |
| `APPLE_NOTARY_KEY_P8_BASE64` | Base64-encoded App Store Connect team API private key |
| `APPLE_NOTARY_KEY_ID` | App Store Connect API key ID |
| `APPLE_NOTARY_ISSUER_ID` | App Store Connect API issuer ID |

The release workflow imports the certificate into a temporary keychain, enables hardened runtime, signs app and DMG, validates that app has stable Developer ID designated requirement, notarizes DMG, staples ticket, and runs Gatekeeper assessment. Missing credentials fail release before artifact upload.

First Developer ID-signed release has different identity from old ad-hoc releases. Existing users must remove old Entropy entries from Accessibility and Input Monitoring, add current app again, and grant permissions once. Later releases signed with same Developer ID preserve that identity.

Inspect built app identity with:

```bash
codesign -dv --verbose=4 Entropy.app
codesign -d -r- Entropy.app
```

Expected output includes Developer ID Application authority and Team ID. Designated requirement must not be `cdhash`-only.
