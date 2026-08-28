# Changelog

All notable changes to PastePaw will be documented in this file.

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

