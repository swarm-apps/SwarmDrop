## 1. UpgradeLink Account Setup

- [ ] 1.1 Register UpgradeLink account at https://upgrade.toolsetlink.com
- [ ] 1.2 Create **Tauri application** in UpgradeLink dashboard (for desktop)
- [ ] 1.3 Create **APK application** in UpgradeLink dashboard (for Android)
- [x] 1.4 Copy AccessKey (bnJ5md-5YtXhz-i710U8oA), TauriKey (LeRhLvlkcdd1FX0etgOJaw), ApkKey (y1uazDtYlT_UrgDk6UeQmA)
- [ ] 1.5 Configure GitHub Secrets: `UPGRADE_LINK_ACCESS_KEY`, `UPGRADE_LINK_TAURI_KEY`, `UPGRADE_LINK_APK_KEY`
- [x] 1.6 Update package name from `com.gy.swarmdrop` to `com.yexiyue.swarmdrop`

## 2. TypeScript SDK Integration (Shared)

- [x] 2.1 Install `@toolsetlink/upgradelink-api-typescript` dependency
- [x] 2.2 Create `src/commands/upgrade.ts` with SDK client initialization
- [x] 2.3 Implement `checkForUpdate()` using Tauri official `check()` + UpgradeLink strategy
- [x] 2.4 Implement `checkAndroidUpdate()` using UpgradeLink SDK
- [x] 2.5 Create `useUpgradeLinkStore` Zustand store for upgrade state management
- [x] 2.6 Create `UpdateDialog` components (ForceUpdateDialog, PromptUpdateDialog)

## 3. Desktop Implementation

- [x] 3.1 Update `src-tauri/tauri.conf.json` updater endpoints (UpgradeLink endpoint)
- [x] 3.2 Implement `executeDesktopUpdate()` using `@tauri-apps/plugin-updater`
- [x] 3.3 Wire up update check in app startup (root layout)
- [ ] 3.4 Test upgrade scenarios on desktop (manual testing)

## 4. Android Implementation

- [x] 4.1 Add AppUpdate dependency to `src-tauri/gen/android/app/build.gradle.kts`
- [x] 4.2 Add `install_android_update` Rust command in `src-tauri/src/commands/upgrade.rs`
- [x] 4.3 Implement Kotlin `startApkUpdate(url: String)` in `MainActivity.kt`
- [x] 4.4 Add "Install unknown apps" permission handling
- [x] 4.5 Implement `executeAndroidUpdate()` with Tauri Event listeners in `useUpgradeLinkStore`
- [ ] 4.6 Test APK download and installation on Android device (manual testing)

## 5. CI/CD Integration

- [x] 5.1 Review current `.github/workflows/release.yml`
- [x] 5.2 Add `upgradeLink-upload` job (统一同步 Tauri + Android)
  - Tauri: 使用统一 endpoint (app_type: tauri)
  - Android: 使用 apk endpoint (app_type: apk) (after Tauri Android build)
- [x] 5.4 Configure `toolsetlink/upgradelink-action@3.0.2` with correct keys
- [ ] 5.5 Test workflow on feature branch (dry-run) (manual testing)

## 6. Testing & Verification

- [ ] 6.1 Build and publish test version v0.1.3-beta (desktop)
- [ ] 6.2 Build and publish test version v0.1.3-beta (Android)
- [ ] 6.3 Verify UpgradeLink dashboard shows both versions
- [ ] 6.4 Test desktop force update scenario
- [ ] 6.5 Test desktop prompt update scenario
- [ ] 6.6 Test Android APK download and installation
- [ ] 6.7 Test Android force update blocks user

## 7. Documentation & Rollout

- [ ] 7.1 Document UpgradeLink configuration in dev-notes
- [ ] 7.2 Update release checklist with UpgradeLink sync steps
- [ ] 7.3 Document Android versionCode mapping rule
- [ ] 7.4 Announce v0.1.3 release with new update mechanism
