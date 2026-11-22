## Downloads

Pick the archive for your system. Each one holds the `stackel` binary, the README and the
examples, in a directory named after the archive.

| System | Architecture | Archive |
| --- | --- | --- |
| Linux (static, any distribution) | x86_64 | `stackel-VERSION-x86_64-unknown-linux-musl.tar.gz` |
| Linux (static, any distribution) | aarch64 | `stackel-VERSION-aarch64-unknown-linux-musl.tar.gz` |
| Linux (static, any distribution) | armv7 (hard float) | `stackel-VERSION-armv7-unknown-linux-musleabihf.tar.gz` |
| Linux (static, any distribution) | i686 | `stackel-VERSION-i686-unknown-linux-musl.tar.gz` |
| Linux (glibc) | riscv64 | `stackel-VERSION-riscv64gc-unknown-linux-gnu.tar.gz` |
| Linux (glibc) | powerpc64le | `stackel-VERSION-powerpc64le-unknown-linux-gnu.tar.gz` |
| Linux (glibc) | s390x | `stackel-VERSION-s390x-unknown-linux-gnu.tar.gz` |
| Linux (glibc) | loongarch64 (experimental) | `stackel-VERSION-loongarch64-unknown-linux-gnu.tar.gz` |
| macOS | Apple silicon | `stackel-VERSION-aarch64-apple-darwin.tar.gz` |
| macOS | Intel | `stackel-VERSION-x86_64-apple-darwin.tar.gz` |
| Windows | x86_64 | `stackel-VERSION-x86_64-pc-windows-msvc.zip` |
| Windows | i686 | `stackel-VERSION-i686-pc-windows-msvc.zip` |
| Windows | arm64 | `stackel-VERSION-aarch64-pc-windows-msvc.zip` |

`VERSION` is the version number without the `v`. Archives that are missing from the assets
below failed to build for this release.

## Verify

Download `SHA256SUMS` next to the archive and run

```sh
sha256sum --check --ignore-missing SHA256SUMS   # Linux
shasum -a 256 --check --ignore-missing SHA256SUMS   # macOS
```

(On Windows: `Get-FileHash <archive>` and compare with the line in `SHA256SUMS`.)
Every binary was run against all of the examples on its own platform (or under QEMU for the
other architectures) before it was published.
