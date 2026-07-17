# Native Build Notes

SwarmDrop Mobile follows the sibling-project layout used by `swarmnote-mobile`.

## Android

```bash
pnpm install
pnpm prebuild
pnpm --filter react-native-swarmdrop-core build:android
pnpm android
```

## iOS

```bash
pnpm install
pnpm prebuild
pnpm --filter react-native-swarmdrop-core build:ios
pnpm ios
```

The generated `android/` and `ios/` folders are intentionally ignored until the native bridge is ready.
