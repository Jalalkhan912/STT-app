Vendored from https://github.com/rustdesk-org/rdev at rev 0904f23ce496e255461a98848b85c11fa5a62eab.

Local change: src/macos/listen.rs re-enables the event tap (CGEventTapEnable(tap, true))
when macOS delivers TapDisabledByTimeout or TapDisabledByUserInput. Upstream only logs it.
