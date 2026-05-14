# SwarmDrop Mobile

React Native mobile client for SwarmDrop, using Expo Router and a development build.

## Commands

```bash
pnpm install
pnpm start
pnpm android
pnpm ios
pnpm typecheck
```

Expo Go is not supported because the app will load native Rust bindings through `react-native-swarmdrop-core`.

Native projects are generated with:

```bash
pnpm prebuild
```

Build the Rust UniFFI package before running native builds:

```bash
pnpm --filter react-native-swarmdrop-core build:android
pnpm --filter react-native-swarmdrop-core build:ios
```

On Windows, use a short Rust target directory to avoid linker failures from long paths:

```powershell
$env:CARGO_TARGET_DIR='D:\tmp\swarmdrop-mobile-core-target'
pnpm --filter react-native-swarmdrop-core build:android
```
