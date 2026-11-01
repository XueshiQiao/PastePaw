# Changelog

All notable changes to PastePaw will be documented in this file.

## v1.6.0

### Changed
- **Brand-New App Icon (Unified with macOS)**: Refreshed the Windows application icon across all resolutions with the vibrant sky-blue cat paw design, aligning PastePaw's visual identity with the latest macOS version.
- **Smooth Squircle Card Design**: Upgraded clipboard cards and drag previews to G2 continuous-curvature superellipse corners (`corner-shape: squircle`) for softer, more natural corner transitions, with automatic fallback on older WebView2 runtimes.
- **Unified Card Footer Metadata**: Redesigned and standardized bottom-left card footer labels across all 5 languages to cleanly display content type and character/file size (`Text`, `Rich Text`, `Image`).

### Added
- **Rich Text & HTML Formatting Support**: Automatically captures and restores Rich Text (`RTF`) and `HTML Format` alongside plain text when copying from browsers, Office apps, and rich editors (#16).
- **Formatting Controls**: Added a "Preserve Rich Text & HTML Formatting" toggle in Settings (enabled by default) and a "Paste as Plain Text" option in the card context menu (#16).
- **Configurable Move to Top on Paste**: Added a "Move to Top on Paste" setting (enabled by default) to control whether pasting an existing history item bumps it to the top of the list (#12).

### Fixed
- **Auto-Paste Focus & Modifier Reliability**: Explicitly restores foreground focus to the target application window after hiding PastePaw and waits for physical modifier keys to release before simulating `Shift + Insert` / `Ctrl + V` (#13).
- **Paste List Sync & Image Metadata**: Fixed clipboard list synchronization when bumping pasted items to the top, reset card selection cleanly on window reopen, and preserved original `source_app` metadata when pasting images (#12).
- **Updater Release Notes**: Passed extracted changelog notes into CI updater artifact generation so `latest.json` includes version release notes.

### 修改
- **全新应用图标（与 macOS 版保持一致）**：全面换新 Windows 版各尺寸应用图标，采用与最新 macOS 版一致的天空蓝渐变猫爪设计，统一跨平台视觉形象。
- **超椭圆平滑圆角卡片（Squircle）**：剪贴板卡片与拖拽预览全面升级为 G2 曲率连续的超椭圆平滑圆角（`corner-shape: squircle`），消除直线与圆角衔接处的生硬拐点，视觉过渡更圆润自然（同时对旧版 WebView2 提供自动圆角兜底）。
- **统一卡片左下角信息标签**：重新梳理并统一所有卡片左下角的信息角标样式（支持 5 国语言），清晰展示“文本 (N 个字符)”、“富文本 (N 个字符)”与“图片 (N KB)”。

### 新增
- **保留富文本与 HTML 格式**：从浏览器、Office 文档或富文本编辑器复制内容时，支持同时捕获并在粘贴时还原 `RTF` 与 `HTML` 富文本格式 (#16)。
- **富文本控制项**：在设置页新增“保留富文本与 HTML 格式”开关（默认开启），并在卡片右键菜单中新增“粘贴为纯文本”选项 (#16)。
- **粘贴后自动置顶开关**：在设置页新增“粘贴后自动置顶”配置项（默认开启），可自由控制粘贴历史记录后是否将其移至列表最前面 (#12)。

### 修复
- **自动粘贴焦点与修饰键可靠性**：隐藏主窗口后显式恢复目标应用的前台窗口焦点，并在模拟 `Shift + Insert` / `Ctrl + V` 前等待物理修饰键释放，避免快捷键粘连导致粘贴失效 (#13)。
- **粘贴置顶同步与图片元数据保留**：修复粘贴置顶后前端列表即时刷新与唤出窗口时的首项选中重置逻辑，并避免粘贴图片时覆盖原始来源应用（`source_app`）信息 (#12)。
- **应用内更新日志显示**：修复 CI 构建 `latest.json` 时未注入版本更新说明的问题。

## v1.5.0

### Added
- **Multi-size Display Layout**: Introduced customizable UI sizing options ("Default" and "Compact") for better use of screen real estate on smaller displays. The window resizes instantly upon selection.
- **Background Update Check**: Added a non-intrusive UI indicator for new updates in the main window and a direct "Update" button in the Settings footer (#15).

### Changed
- Refined the default clipboard card aspect ratio to be more perfectly square for improved aesthetics.
- Optimized text contrast in Dark Mode to ensure the app name header remains visible on vibrant colored backgrounds.
- Unified toolbar icon colors (Add, Update, Settings) to adapt seamlessly to the system theme.

### Fixed
- Fixed sub-pixel rendering gaps and black borders around selected cards in specific display scales.
- Restored robust Mica effect persistence across window toggling, system sleep, and wake cycles (#9, #10).
- Fixed redundant UI state updates causing visual tearing when deleting items via context menus (#14).
- Added reliable image cleanup from the filesystem before cascaded folder deletions in SQLite (#11, #14).

### 新增
- **多尺寸界面布局**：加入可自定义的“界面大小”选项（默认 / 紧凑），紧凑模式专为小屏设备优化，切换时主窗口即时丝滑缩放。
- **后台更新检测**：增加后台静默更新检测机制，在主界面显示无打扰更新图标，并在设置页底部提供直接更新按钮 (#15)。

### 修改
- 优化默认剪贴板卡片的宽高比例，使其视觉上更接近完美的正方形，提升整体美感。
- 增强了深色模式下的文字对比度，确保卡片来源应用的标题在鲜艳背景上依旧清晰可读。
- 统一主界面工具栏图标（添加、更新、设置）颜色，使其与系统主题完美适配。

### 修复
- 修复了在部分缩放比例下，选中卡片时边缘由于亚像素渲染（Sub-pixel rendering）导致的黑色缝隙问题。
- 修复了窗口频繁呼出、系统睡眠唤醒后 Mica 材质特效丢失的问题 (#9, #10)。
- 修复了通过右键菜单删除项目时，冗余的状态更新导致的视觉闪烁问题 (#14)。
- 完善了 SQLite 级联删除机制，彻底解决了删除目录前残留实体图片文件的问题 (#11, #14)。



## v1.3.8

### Added
- Configurable Auto-Paste shortcut method (Shift + Insert / Ctrl + V) with extended virtual key event simulation (#13)
- Confirmation dialog when deleting folders, with cascading deletion of folder clips and associated images (#14)
- Option to preserve saved folder items when clearing clipboard history (#11)

### Fixed
- Fixed multi-monitor and mixed-DPI display coordinate scaling issues (#9)
- Fixed backdrop material (Mica / Mica Alt / Clear) and dark theme persistence across system sleep, wake, and lock screen (#10)
- Fixed default selection and scroll reset to the first clip upon window reopen or receiving new clips (#12)
- Fixed clip list falling into an empty state when deleting the currently selected folder by automatically switching back to All Clips (#14)

### 新增
- 支持在设置中选择自动粘贴快捷键模式（Shift + Insert / Ctrl + V），并增强虚拟按键事件模拟与终端兼容性 (#13)
- 删除文件夹时增加确认对话框，并支持级联删除文件夹内的历史项及磁盘关联图片 (#14)
- 清空剪贴板历史时默认保留已保存至文件夹中的项目 (#11)

### 修复
- 修复多显示器及不同 DPI 缩放下的窗口定位与尺寸错位问题 (#9)
- 修复在系统休眠、锁屏唤醒后 Mica/Mica Alt/Clear 材质与深色主题属性失效的问题 (#10)
- 修复重新唤出窗口或接收新剪贴板内容时未自动定位并选中第一项的问题 (#12)
- 修复删除当前选中的文件夹后未自动回退至"全部历史"导致列表显示为空的问题 (#14)

## v1.3.7

### Added
- German, French, and Japanese language support

### Improved
- Winget release pipeline: hash verification step added before publishing to winget-pkgs to prevent stale-hash mismatches; release tag now explicitly pinned

### 新增
- 新增德语、法语、日语语言支持

### 优化
- Winget 发布流程：在发布至 winget-pkgs 前增加哈希值校验步骤，防止哈希不匹配问题；发布时明确指定 release tag

## v1.3.6

### Added
- Support floating window above the taskbar (toggle in Settings)
- Every release is now automatically scanned with VirusTotal (70+ antivirus engines) — scan results are linked in the release notes

### 新增
- 窗口支持浮动在任务栏上层（可在设置中开启/关闭）
- 每次发布版本现在会自动通过 VirusTotal（70+ 款杀毒引擎）进行安全扫描，扫描结果链接附在 Release 说明中

## v1.3.5

### Added
- Native rounded corners support for all window effects (Mica, Mica Alt, Clear) using Windows 11 DWM — toggle on/off in Settings

### Fixed
- Fixed TypeScript build error caused by missing Vite client types (`import.meta.env`)

### 新增
- 所有窗口效果（Mica、Mica Alt、Clear）均支持原生圆角，通过 Windows 11 DWM 实现，可在设置中开启/关闭

### 修复
- 修复因缺少 Vite 客户端类型导致的 TypeScript 构建错误（`import.meta.env`）

## v1.3.4

### Added
- Brand new native style look with Windows Mica and Mica-Alt window effects for a seamless, beautiful appearance that blends with your desktop

### 新增
- 全新原生风格外观，支持 Windows Mica 和 Mica-Alt 窗口效果，与桌面完美融合，带来更精美的视觉体验

## v1.3.3

### Changed
- Refined UI layout: reduced window height, tightened card spacing, fixed control bar height, and removed CSS shadow in Clear window effect mode

### 变更
- 优化界面布局：减小窗口高度、收紧卡片间距、固定控制栏高度，并在"无效果"窗口模式下移除 CSS 阴影

## v1.3.2

### Fixed
- Fixed hotkey toggle broken after changing hotkey in settings (issue #6)
- Fixed winget package missing arm64 installer by switching to NSIS setup.exe for architecture detection (issue #7)

## v1.3.1

### Fixed
- Removed white/alpha border around settings window in dark mode

