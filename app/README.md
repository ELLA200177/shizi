# 拾字 · M1 骨架

> 更新 2026-10-10。对应 [../PRD-v1.md](../PRD-v1.md) 第 9 节。

## 现状

| 部分 | 状态 |
|---|---|
| 词条库 | ✅ **153 条**（化学 85 / 语言 67 / 育种 1），来自 `../packs/*.json`（医学包 54 条已移出，文件保留） |
| 检索核心 `ui/search-core.js` | ✅ 完成，**34 个回归用例全通过** |
| 搜索面板 UI | ✅ 完成，极简卡片式；可直接在浏览器预览 |
| 配色主题 | ✅ 7 套（默认「极简白」），可现场切换对比 |
| 原生层（托盘 / 热键 / 注入） | ✅ **M1 完成**，已实测通过（见下） |

## M1 原生层（2026-10-10 完成）

| 能力 | 状态 |
|---|---|
| 托盘常驻 | ✅ 左键单击 / 右键菜单打开面板 |
| **单实例锁** | ✅ 第二个实例会唤出已有面板后自行退出（2026-10-12）。没有它，双击两次托盘图标就会起两套：两个图标、两个键盘钩子、两份词库连接 |
| 开机自启 | ✅ 走 `tauri-plugin-autostart`，设置页可开关。状态以**系统实际值**为准，不信数据库 || 全局热键 | ✅ **候选降级**：Ctrl+Alt+Space → Ctrl+Shift+Space → Ctrl+Alt+Q → Alt+K |
| 无边框面板 | ✅ 582×500，屏幕中央偏上，Windows 11 圆角（DWM） |
| 失焦自动隐藏 | ✅ |
| 剪贴板注入 | ✅ 带序列号校验的剪贴板还原（不覆盖用户新复制的内容） |
| Unicode 兜底 | ✅ `KEYEVENTF_UNICODE`，按 **UTF-16 码元**逐个发送 |
| 词条来源 | **SQLite**：`%APPDATA%\com.shizi.app\shizi.db`（153 条 / 3 个包已导入） |
| **词库包导入导出** | ✅ 导出自己的词条 / 任意包为 JSON；导入前先预览冲突，撞车的触发码只清码不丢词 |
| **设置页** | ✅ 在管理窗口左栏底部：热键切换、展开开关、开机自启、数据目录、隐私说明 |
| **首启引导** | ✅ 第一次运行自动打开欢迎卡片，说明热键 / 触发码 / 怎么加词 || 触发码来源 | 由 `packs/*.json` 直接携带（127 个），经 `normalize_packs.py` 写回 |
| 触发词 | ✅ 面板行首显示；127 条有触发码，26 条（符号表类）只能搜索 |

### 数据流（2026-10-10 起）

```
packs/*.json  ──normalize_packs.py──►  packs/*.json（触发码写回，此处是唯一权威）
      │                                        │
      │                              include_str! 编译期嵌入
      ▼                                        ▼
ui/entries.js（仅供浏览器预览）          SQLite  shizi.db（153 条）  ──load_entries──► 面板
                                               ▲
                                    用户自己的词条写这里（下一步做界面）
```

**改词条的两种情形，代价不同：**

| 改什么 | 怎么做 | 要重新编译吗 |
|---|---|---|
| **用户自己的词条** | 写数据库 | ❌ 不用 |
| **官方内置包** | 改 `packs/*.json` → `normalize_packs.py` → **提升包版本号** → `cargo build` | ✅ 要（包是编译期嵌入的） |

> 内置包版本号变了就会重导，且**只删 `origin='pack'` 的行**，用户词条不受影响。

### 实测结果

```
[拾字] 热键已就绪：Ctrl+Alt+Space
窗口: VISIBLE title='拾字'  pos=562,171  size=597x509   前台窗口=拾字

注入回归（PRD 附录 B.2 的 7 个码点边界用例）:
OK   delta13C         [U+03B4 U+00B9 U+00B3 U+0043]
OK   astral U+20BB7   [U+D842 U+DFB7]
OK   micro U+00B5     [U+00B5]
OK   celsius U+2103   [U+2103]
OK   cjk 6 chars      [U+809D U+8C46 U+72B6 U+6838 U+53D8 U+6027]
OK   emdash x2        [U+2014 U+2014]
OK   nbsp U+00A0      [U+00A0]
RESULT cases=7 failed=0
```

### ⚠️ 开发参数与单实例锁的交互

单实例插件在 `main.rs` 里是**第一个**注册的插件（它必须在其余初始化之前决定进程走留）。
副作用：**已经有一个拾字在跑时，带开发参数的启动会被拦掉**。
跑 `--show-manager` / `--selftest-manager-close` / `--dump-entries` 之前，先关掉正在运行的实例。

### 依赖的 Tauri 插件

| 插件 | 用途 |
|---|---|
| `tauri-plugin-single-instance` | 单实例（**必须第一个注册**） |
| `tauri-plugin-global-shortcut` | 全局热键 |
| `tauri-plugin-dialog` | 导入导出的文件选择框 |
| `tauri-plugin-autostart` | 开机自启 |

其他直接依赖：`rusqlite`（bundled SQLite）、`pinyin`（触发码建议）、`arboard`（剪贴板）、`serde`。

### 怎么跑

```bash
cd app/src-tauri
cargo run                      # 托盘常驻，按 Ctrl+Alt+Space 唤起面板
cargo run -- --show-now        # 启动即弹出面板（无键盘环境用）
cargo test --bin shizi         # 单元测试：FFI 结构体布局 / 代理对拆分
cargo test -- --ignored        # 运行时 FFI 测试（会真的往焦点窗口发一次 Ctrl+V）
cargo build --release          # 发布构建
```

```powershell
# 注入回归测试。必须用 Windows PowerShell 5.1 —— pwsh 下 WinForms 程序集加载会失败
C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -ExecutionPolicy Bypass ^
  -File app/tools/inject_regression.ps1
```

### 触发词方案（2026-10-10 补）

面板每行行首显示该词条的触发码（等宽字体、灰色），选中时变蓝；触发码也进了检索串，所以输入 `;gdzxbx` 或 `gdzxbx` 都能命中。

生成规则（`gen_entries.py`，对应 PRD 7.4）：

1. **源包里显式写了 `trigger` 的优先**，算法不覆盖用户既有习惯（例：育种那条显式声明 `;kx`）
2. 否则按 **拼音首字母 → 全拼 → 无空格的 ASCII 别名** 依次尝试
3. **全局唯一**；撞车就顺延到下一个候选
4. **候选若是别的词条的别名，直接跳过** —— 否则搜这个码会被那条别名以同等权重抢走
5. 都不行则留空（26 条符号表类），面板不显示，这类词条只能搜索

实测的 5 处撞车都被自动化解：

| 原候选 | 分配结果 |
|---|---|
| `qg` | 髂骨 `;qg` ／ 颧骨 `;quangu`（降级到全拼） |
| `fg` | 腓骨 `;fg` ／ 跗骨 `;fugu` |
| `gj` | 膈肌 `;gj` ／ 疳积 `;ganji` |
| `kx` | **系谱 `;kx`（显式保留）** ／ 咯血 `;kaxue` |
| `zhuyin` | 注音组 11 条共享该别名 → 一律不给触发码，避免搜到别的条目 |

**2026-10-10 补**：24 个小写希腊字母已拆成独立条目（各有 `;alpha`…`;omega`），
原先它们只在一条「24 字母全表」里，搜 `sigma` 只能拿到整表。拆开后按英文名搜索直接命中单个字母。

两个字母需要**显式声明**触发码 —— 因为它们的候选名已被别的词条占用：

| 字母 | 候选 | 被谁占了 | 处理 |
|---|---|---|---|
| ξ | `xi` | `郗`（language.zh.0052，读 xī） | 显式声明 `;xi` |
| χ | `chi` | `喫 / 吃`（language.zh.0037） | 显式声明 `;chi` |

> 这正是 PRD 7.4 里那个「跨领域触发码必然冲突」的又一个真实案例：
> 希腊字母名与中文姓氏/异体字的拼音撞车。自动规则选择退让（不给码），
> 而用户明确需要时用显式声明覆盖。

回归测试已扩到 **32 + 3 项触发词断言**，含「127 个触发码都能搜到自己」。

### 设计要点

1. **手写 Win32 FFI，不用 `windows` crate** —— 只需要 6 个函数，自己声明没有版本漂移风险，
   且结构体布局被 `input_layout_matches_win32` 测试锁住（布局错位会让 `SendInput` 静默失败，极难排查）。
2. **热键候选降级** —— 写死一个热键等于"用户装上就崩"。本机 `Alt+Space`/`Win+Space` 都已被占用，
   所以按优先级逐个试，界面底部显示实际生效的那个；一个都没注册上会明确提示用托盘图标，**不静默失败**。
3. **开发自测入口** —— `--inject-file` 让注入模块能在没有键盘、没有面板的情况下被自动化测试，
   这正是 7 个码点用例能跑起来的原因。

### 已知问题 / 未验证

- **多显示器下面板位置可能不对**：定位用的是 primary monitor 的逻辑尺寸，实测落在 (562,171)，
  与公式推算略有偏差，需要在双屏下校准
- **提权窗口注入未验证**：以管理员运行的 Word 等目标是否成功，PRD 9.4 第 1 条要求实测
- 面板**不透明**，桌面级毛玻璃（DWM acrylic）未做
- 词条仍在前端硬编码，未接 SQLite 与包管理（属 M2）

## 性能实测基线

体积、内存、与 Espanso 的对比见仓库根目录的 **[perf-baseline.md](../perf-baseline.md)**
（含测量口径说明与复现命令）。

一句话：**体积 7.7 MB 优于 Espanso 的 23.3 MB；内存 162 MB 远差于它的 7.2 MB** —— 
差距是 WebView2 的固定底噪（约 107 MB）决定的，不是实现质量。

## 工具链（2026-10-10 已就位）

**选型 B（Tauri）已具备条件**，全部安装在 D 盘。

| 组件 | 版本 | 位置 |
|---|---|---|
| rustc / cargo | 1.99.0 `stable-x86_64-pc-windows-msvc` | `D:\Rust\cargo` |
| MSVC 工具集 | 14.44.35207 | `D:\BuildTools\VC\Tools\MSVC\` |
| Windows SDK | 10.0.26100.0 | 系统默认路径 |
| WebView2 运行时 | 154.0.4258.62 | 系统 |
| Node / pnpm | v24.19.0 / 11.25.0 | — |

环境变量（已持久化到用户级）：`RUSTUP_HOME=D:\Rust\rustup`、`CARGO_HOME=D:\Rust\cargo`、`PATH` 含 `D:\Rust\cargo\bin`。

**已实测通过**：`cargo build` 能编译、链接并运行出可执行文件；`cargo add` 能从镜像下载第三方 crate。

### 安装命令（可复现）

```powershell
# 1) rustup -> D 盘（不需要管理员）
$env:RUSTUP_HOME = 'D:\Rust\rustup'
$env:CARGO_HOME  = 'D:\Rust\cargo'
[Environment]::SetEnvironmentVariable('RUSTUP_HOME', $env:RUSTUP_HOME, 'User')
[Environment]::SetEnvironmentVariable('CARGO_HOME',  $env:CARGO_HOME,  'User')
$env:RUSTUP_DIST_SERVER = 'https://rsproxy.cn'
$env:RUSTUP_UPDATE_ROOT = 'https://rsproxy.cn/rustup'
curl.exe -L -o "$env:TEMP\rustup-init.exe" https://rsproxy.cn/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
& "$env:TEMP\rustup-init.exe" -y --default-toolchain stable --profile default

# 2) VS Build Tools -> D 盘（需要管理员；会弹 UAC）
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --source winget `
  --accept-package-agreements --accept-source-agreements --disable-interactivity `
  --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --installPath D:\BuildTools"
```

实测耗时约 30 分钟（其中大部分是 `--includeRecommended` 附带装 CMake/Ninja/ATL 等组件）。
安装完成后「添加或删除程序」里显示为 **Visual Studio 生成工具 2022 17.14.41**。

> 如果只想要最小集，可以去掉 `--includeRecommended`，改为显式指定 Windows SDK 组件，能省 1GB 左右和一半时间。

### 三个坑（换机器会再遇到，记下来）

1. **Rust 官方源在国内基本不可用。** 实测 `static.rust-lang.org` 8 秒只给 47KB，`mirrors.tuna` 超时，`mirrors.ustc` 返回 403。
   最终用 **`rsproxy.cn`**（字节跳动）：2.25 秒下完 12MB。
   - rustup：`RUSTUP_DIST_SERVER=https://rsproxy.cn`、`RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup`
   - cargo：`D:\Rust\cargo\config.toml` 里把 crates.io 替换为 `sparse+https://rsproxy.cn/index/`
2. **winget 会先探 `msstore` 源**，国内连不通时报 `WinHttpSendRequest: 12029` 并直接中断安装。**必须加 `--source winget`**。
3. **自定义安装路径可行**：`--override "... --installPath D:\BuildTools"` 生效，且 `vswhere` 能正确注册该路径 —— rustc 仍能找到 `link.exe`（这一点原本担心会失败，实测没问题）。

> 探针工程 `D:\Rust\_probe_*` 可以删掉，只占几 MB。

## 环境陷阱（重要，2026-10-10 实测）

### 1. Codex 是 MSIX 打包应用，`%APPDATA%` 写入会被沙箱重定向

从 Codex 里启动的进程，对 `%APPDATA%` / `%LOCALAPPDATA%` 的写入**可能被重定向**到：

```
%LOCALAPPDATA%\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Roaming\
```

**后果**：我写进 `%APPDATA%\espanso\match\` 的词条文件实际落在沙箱里，而用户自己从开始菜单启动的 Espanso 读的是**真实路径**，读不到——M0 实验曾经是空转的。

**怎么发现**：用 WMI 派生进程来验证（`Invoke-CimMethod Win32_Process Create`）。WMI 由系统服务创建进程，**不在包上下文里**，能看到真实路径：

```powershell
Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
  CommandLine = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "D:\probe.ps1"'
}
```

**规则**：凡是写用户配置目录（Espanso/拾字的 %APPDATA%），写完必须用 WMI 派生进程反查一次真实性。工作区（`Documents\...`）不受影响。

### 2. 全局热键可用性实测（本机）

用 `RegisterHotKey` 逐个试（注册后立刻 `UnregisterHotKey`，不留痕）：

| 组合 | 结果 |
|---|---|
| `Alt+Space` | **被占用**（Win32 错误 1409 = ERROR_HOTKEY_ALREADY_REGISTERED） |
| `Win+Space` | 被占用（系统输入法切换，正常） |
| `Ctrl+Alt+Space` | 可用 |
| `Ctrl+Shift+Space` | 可用 |
| `Alt+Q` / `Alt+K` / `Alt+;` | 可用 |

**拾字的默认热键定为 `Ctrl+Alt+Space`**：与 Alt+Space 手感最接近，且实测无冲突。

**已在真实环境验证**：把 Espanso 的 `search_shortcut` 改成 `CTRL+ALT+SPACE` 后重启，日志里原本每次启动都出现的 `[ERROR] unable to register hotkey: ALT+SPACE` **消失了**。
这比用短命进程试 `RegisterHotKey` 更可信——Espanso 是长期驻留、带完整消息循环的守护进程，与拾字面板的形态一致。

> `Alt+Space` 被谁占了：Windows 没有查询接口。候选是 ChatGPT/Codex 桌面版（11 个进程）与豆包 DoubaoWork（配置里有 text_picker 划词功能）。Codex 日志显示它注册的是 `DoubleAlt`（Appshot 热键），所以更可能是豆包或其他常驻程序。
> 注意：`Alt+Space` 同时还是 Windows 内建的「窗口系统菜单」（还原/移动/大小/关闭），但那是系统行为，**不是** 1409 的原因，也没有可删除的注册项。

### 2.35 `packs.source` 与 `entries.origin` —— 两张表两种叫法

**症状**：导入词库包时报 `no such column: origin in SELECT origin FROM packs`。

**根因**：两张表的同类字段叫法不同，写代码时混用了。

| 表 | 字段 | 取值 |
|---|---|---|
| `packs` | **`source`** | builtin / imported |
| `entries` | **`origin`** | pack / user / imported |

这个错误是**导出导入往返测试**抓到的 —— 不写那条测试，它会在用户点「导入」时才炸。
源码里已加防混注释。

### 2.4 触发码曾经只有 4 个 —— 两份真相源的代价

**症状**：数据库导入 153 条后，`SELECT COUNT(*) FROM entries WHERE trigger<>''` 只返回 **4**。

**根因**：触发码的生成逻辑**只活在 `gen_entries.py`（Python）里**，它写进前端的 `entries.js`，
但**没有写回 `packs/*.json`**。而数据库是从 packs 导入的 —— 于是库里几乎没触发码，
面板不显示触发码、打字自动展开也无从匹配。

**修法**：新增 `app/tools/normalize_packs.py`，把触发码**写回包文件**，让 `packs/*.json`
成为唯一权威数据；`gen_entries.py` 退化为纯搬运，不再有第二份实现。

**为什么能漏掉**：没有任何测试检查「包里的触发码数量」。现在有了 ——
`store::tests::builtin_packs_carry_their_triggers` 会断言触发码 ≥100 个且关键码存在。

> 顺带：这次排查中我写坏过一次 `chemistry.zh.json`（打开文件截断后才去读它，等于清空），
> 已用 `entries.js` 重建。教训是**先读后写**，`normalize_packs.py` 里也加了这条约束。

### 2.4b 面板一打开就是空的 —— 空查询返回的数据结构不一致

**症状**：面板唤起后列表全空；只要开始打字就正常。用户最早的抱怨
「搜索框里没有出现词条的触发词」就是它。

**根因**：`search-core.js` 的**空查询分支**直接返回词条对象，
而普通查询返回 `{entry, weight, score}`；调用方统一按 `r.entry` 取值 —— 于是抛
`Cannot read properties of undefined`，整个 render 中断。

**修法**：空查询也返回同构结果。测试补了「F. 空查询」三段断言。

### 2.5 「隐身面板霸占系统前台」—— 按回车不插入的真凶

**症状**：面板能唤起、能搜索，按回车却什么都不插入。

**根因**：Tauri 窗口配置写成 `"visible": false` 时，窗口本身确实不可见
（`IsWindowVisible` 为 false），**但它依然成了系统的前台窗口**：

```
拾字启动后（未按任何热键）:
  hidden  class='Tauri Window' title='拾字'
当前前台窗口:
  pid=<拾字>  class='Tauri Window' title='拾字'     ← 一个看不见的窗口占着前台
```

后果有两层：

1. 用户一启动拾字就**丢掉正在用程序的前台**（他自己察觉不到，因为抢走它的窗口是隐形的）
2. 此后所有注入的 `Ctrl+V` 都粘回拾字自己的隐藏面板 —— 于是"什么都没发生"

**修法**：窗口配置加 `"focus": false`。修完后：

```
启动后前台窗口 = 0xdd0b90 class=Chrome_WidgetWin_1 属于自己=false   ← 不再抢前台
抢焦点结果: true (直接 SetForegroundWindow)                        ← 不再需要 Alt 兜底
```

另外加了**自指护栏**：`toggle_panel` 捕获前台窗口后会检查它是否属于自己进程，
是则丢弃并告警，杜绝"把文本粘回自己"。

### 2.6 Windows 前台锁：为什么 `SetForegroundWindow` 会静默失败

面板是前台窗口时，`SetForegroundWindow(目标)` 会被 Windows 拒绝（返回 0，不报错）。
实测三条退让路径的可用性：

| 路径 | 结果 |
|---|---|
| 直接 `SetForegroundWindow` | **在「先隐藏面板」的顺序下必然失败** |
| `AllowSetForegroundWindow(ASFW_ANY)` 后再抢 | 仍然失败（许可只对发放者有效期内、且要看时序） |
| `AttachThreadInput` 后抢 | 失败 |
| **先发一次 Alt 键解锁后再抢** | ✅ 成功 |

**但要注意 Alt 解锁的副作用**：`keybd_event` 注入的 Alt 若不清扫，会让后续
`Ctrl+V` 变成 `Ctrl+Alt+V`（表现为"焦点对了但粘贴不动"）。所以解锁后必须补发
Alt / Ctrl / Shift 的 keyup 清扫。

**最终采用的顺序**（实测有效）：

1. 趁面板**还是前台**时调 `AllowSetForegroundWindow(ASFW_ANY)`
2. 隐藏面板 —— 由系统把前台**和内部焦点控件**一起还给目标（若先抢前台再隐藏，内部焦点会丢）
3. 抢前台（此时多半能直接成功）
4. 发 `Ctrl+V`

### 2.7 Tauri 关窗口是「销毁」，不是「隐藏」

**症状**：管理窗口被关掉一次之后，点托盘的「管理词库…」再也打不开，只能重启程序。
日志里反复出现 `找不到管理窗口`。

**根因**：Tauri 里用户点右上角的 X 会**销毁**窗口，而窗口只在启动时按配置创建一次。
销毁之后 `get_webview_window("manager")` 永远返回空，`show()` 无从谈起。

**修法**：在 `on_window_event` 里拦 `CloseRequested` → `api.prevent_close()` + `win.hide()`。

> ⚠️ **只拦需要复用的那一个窗口。** 拦多了会把「退出」也挡住。
> 本项目写成 `if win.label() == "manager"`，并且加了 `--selftest-manager-close` 自动验证三件事：
> 关掉后窗口仍存在、能重新打开、**程序仍能正常退出**。
>
> 更一般地说：**"窗口能关掉"和"窗口能复用"是两种设计**，代码里必须显式选一种，
> 不能靠默认行为——默认行为在这里恰好是用户最容易踩到的那种。

### 3. 两个 Espanso 实例会抢命名管道

沙箱实例和真实实例都用 `\\.\pipe\espansodaemonv2`。先起的占住管道，后起的直接崩：

```
[ERROR] unable to initialize ipc server for daemon: 拒绝访问。(os error 5)
[ERROR] unable to initialize ipc server: 拒绝访问。(os error 5)
```

**后果**：`service restart` 看着像成功了（有进程在跑），但跑的其实是**旧实例读旧配置**——改了配置却"没生效"，就是这个原因。

**正确做法**：先 `service stop`，确认 `Get-Process espansod` 归零，再启动。强杀也行（`Stop-Process -Force`），因为 daemon 与 worker 是成组的，杀掉一个另一个会跟着退出。

### 4. 「打开数据目录」打不开 —— 真凶是 MSIX 虚拟存储，不是隐藏目录（2026-10-12）

这一节记录一次**诊断错误及其更正**，因为踩的坑本身值得留着。

**现象**：点设置页的「打开数据目录」，资源管理器毫无反应。

**当时的（错误）结论**：`%APPDATA%` 带隐藏属性，资源管理器拒绝从命令行进入隐藏目录，
于是改成 `explorer.exe /select,<目录>`（只停在父目录 `Roaming` 并选中该文件夹）。

**事实**：用**非打包进程**（`Invoke-CimMethod Win32_Process Create` 派生）查真实文件系统：

| 路径 | 真实存在？ |
|---|---|
| `%APPDATA%\com.shizi.app` | **不存在** |
| `%APPDATA%\espanso` | 存在（对照组） |

数据目录当时只存在于 Codex（MSIX 应用）的虚拟存储：
`%LOCALAPPDATA%\Packages\OpenAI.Codex_*\LocalCache\Roaming\com.shizi.app`。
资源管理器跑在真实上下文，看不到这条路径 → **什么都不打开**，而退出码仍是 0，
于是看起来像"被策略拒绝"。

**为什么会这样**：这一版拾字此前**所有运行都是从 Codex 里启动的**（构建、自测都在同一条链上），
子进程继承了 MSIX 包身份，写 `%APPDATA%` 被重定向进沙箱。用户双击桌面图标运行时才是真实上下文。

**更正后的做法**：`explorer <目录>` 直接打开即可。`AppData` 确实带隐藏属性
（`attrib` 实测为 `H`），但**隐藏属性不是原因**，也不影响导航。已把 `/select,` 改回直接打开。

**怎么验证真实存在性**（别再被骗）：

```powershell
# 非打包上下文 —— 查真实存在性唯一可信的方式
Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
  CommandLine='cmd.exe /c "if exist <路径> (echo YES) else (echo NO)"' }
```

MSIX 打包进程看到的是**合并视图**：它写出来的目录，真实上下文里可能压根不在。
而 `explorer.exe` 会把导航交给真实 shell，所以 CLI 探测到的"打不开"，反映的是真实上下文 ——
这次恰好证明了「路径不存在」，而不是"被拒绝"。

**对用户的实际影响**：数据会落在沙箱里，用户在自己的 AppData 里找不到；
从桌面图标启动（真实上下文）又会读真实路径 —— 两份数据会分叉。
所以：**给用户用的版本不要从 Codex 里启动**；迁移数据要借非打包进程写真实路径。

### 5. 数据目录挪到用户看得见的地方（2026-10-12 起）

`%APPDATA%\com.shizi.app` → **`<文档>\拾字数据\shizi.db`**
（若文档目录被 OneDrive 之类的云同步接管，则退回 `%USERPROFILE%\拾字数据`）。

理由：

- 目标是"不写配置"的用户 —— 词库是他们的资产，必须能自己找到、复制、备份；
- `%APPDATA%` 会被 MSIX 打包进程重定向（见上一节），放这里绕开那一整类坑；
- 不无条件用「文档」，是因为云同步目录里放 SQLite 有损坏风险。

老数据（原来在 `%APPDATA%\com.shizi.app\shizi.db`）需要手动搬一次到新目录 ——
本次已由开发侧搬好，未写自动迁移代码（当时还没有别的用户）。

**待验证**：新的打开动作需要在**真实上下文**（用户双击桌面图标）里跑一次才算数 ——
从 Codex 里启动属于打包上下文，测出来的结果不算。

## 目录

```
app/
├── ui/
│   ├── index.html        面板
│   ├── gallery.html      六套配色同屏对比
│   ├── styles.css        样式（CSS 变量 + 6 套主题）
│   ├── themes.js         主题清单（单一事实来源）
│   ├── search-core.js    检索核心（纯函数，无 DOM 依赖）
│   ├── app.js            面板交互 + 主题切换
│   ├── gallery.js        渲染对比页
│   └── entries.js        自动生成，勿手改
└── tools/
    ├── normalize_packs.py  计算触发码并**写回** packs/*.json（触发码的唯一实现处）
    ├── gen_entries.py      纯搬运：packs → entries.js（只给浏览器预览用）
    ├── test_search.js      检索核心回归测试（32 例 + 希腊字母 + 触发码 + 空查询）
    ├── test_ui.js          UI 一致性测试（主题 / CSS 变量 / 引用）
    ├── test_manager.js    管理界面交互测试（视图切换 / 列表渲染）
    └── inject_regression.ps1  注入回归（7 个码点边界用例）
```

## 面板设计（极简卡片式）

面板**本身就是一张卡片**，卡片语言降维到面板上：

| 卡片语言 | 面板上的对应 |
|---|---|
| 白色圆角卡片 + 细描边 + 柔和投影 | 面板本体：`18px` 圆角、`1px #eceef2` 描边、双层阴影 |
| 顶部蓝色胶囊标签 | 搜索框右侧的**蓝色胶囊计数**（如 `153 条`） |
| 灰色小字标签 → 黑色粗体大标题 | 每行：`15.5px/600 #101828` 词条 → **灰色胶囊**显示分组 |
| 单一蓝色点缀 | 全界面只有蓝色 `#1d6ef5` 一种彩色：匹配高亮、计数胶囊、选中色条 |
| 大量留白 | 行高从 44px 提到 **52px**，内边距加大 |

**刻意没有照搬的部分**：去掉了原来的顶部渐变装饰条（卡片式靠描边和阴影立起来，不靠色条）；没有加时间线（面板里没有时间维度）；没有引入圆形按钮（面板是键盘优先，鼠标操作会打断节奏）。

**与卡片 demo 的区别**：`ui/card-demo.html` 是探索稿（词条库时间线），**不属于产品界面**，保留仅供对照。

## 配色主题

**默认是「极简白」**：白色圆角卡片 + 黑白灰为主 + 单一蓝色点缀（`#1d6ef5`）。

| id | 名称 | 说明 |
|---|---|---|
| `minimal` | **极简白** | **默认**。白卡片、黑白灰、单一蓝 |
| `azure` | 蔚蓝 | 深色 · 蓝 |
| `violet` | 紫罗兰 | 深色 · 紫 |
| `indigo` | 靛蓝 | 深色 · 蓝紫 |
| `cyan` | 青蓝 | 深色 · 冷青 |
| `magenta` | 绯紫 | 深色 · 紫粉 |
| `aurora` | 极光 | 深色 · 三色 |

**怎么挑**：打开 `ui/gallery.html`，七套配色同屏对比（同一份词库、同一个查询）。每个面板都是真的能用的。

**怎么改默认**：改 `ui/themes.js` 里数组的顺序，第一个就是默认主题。
**怎么加新主题**：`styles.css` 加 `body[data-theme="新id"]{ ... }` → `themes.js` 加一条 → `:root` 加 `--dot-新id`。`test_ui.js` 会校验三者对齐、变量引用完整、主题块不缺必需项。

> 切换器只出现在浏览器预览模式（`body.preview`），真实应用里不会出现。

## 怎么预览 UI

直接双击 `ui/index.html`（或拖进浏览器）。

会进入**预览模式**：背景铺一层桌面渐变，面板浮在上面；按 Enter 不会插入文本，而是**退化为复制到剪贴板**，并弹出提示。

接入原生宿主后，宿主会注入 `window.__SHIZI_HOST__ = { commit(text), close() }`，面板自动切到真实插入。

## 怎么改词库

```bash
python app/tools/gen_entries.py     # 改完 ../packs/*.json 后重新生成
```

## 怎么跑测试

```bash
node app/tools/test_search.js   # 检索核心，34 例
node app/tools/test_ui.js       # UI 一致性：主题 / CSS 变量 / 引用完整性
node app/tools/test_manager.js    # 管理界面交互：视图切换 / 列表渲染
```

覆盖三类：

- **归一化**（7 例）——上下标数字、声调、全角、微符号、括号
- **不可见字符显示**（3 例）——NBSP / ZWSP / 全角空格必须有可读标签
- **检索 Top-1**（24 例）——中文片段、全拼、拼音首字母、英文缩写、化学式、生僻字单字

## 数据流

```
../packs/*.json
      │  gen_entries.py
      ▼
  ui/entries.js  ──►  search-core.js (canonical + 打分)  ──►  面板列表
```

## 检索规则（对应 PRD 7.3）

`canonical()` 依次做：小写 → 全角转半角 → 去声调（ü 写作 v）→ **上下标数字归一为 ASCII** → 微符号 µ 归一为希腊 μ → 去空白与标点

打分：

| 匹配 | 权重 |
|---|---|
| 完全等于词条文本 | 1.00 |
| 完全等于某个别名 | 0.95 |
| 完全等于拼音首字母 | 0.90 |
| 前缀命中（拼音 / 文本 / 别名） | 0.80 |
| 出现在检索串任意位置 | 0.60 |

`score = 1000×权重 + 200×使用频率 + 300×置顶 − 0.5×文本长度`

## 一个具体的差异化点（可直接写进 README 宣传）

**Espanso 搜不到 `13c`。**

Espanso 的搜索是关键词/缩写匹配，上下标字符不做归一。想让它命中 `δ¹³C`，你得手工给每条加 `search_terms`。

拾字的 `canonical()` 会把 `¹²³` 和 `₁₂₃` 归一成 ASCII 数字，所以这些都是零配置直接可搜：

| 输入 | 命中 |
|---|---|
| `13c` | `δ¹³C` |
| `h2o` | `H₂O` |
| `caco3` | `CaCO₃` |
| `hba1c` | `HbA₁c` |
| `spo2` | `SpO₂` |

**为什么这很重要**：化学式与下标组合是无穷的（`C₆H₁₂O₆`、`PO₄³⁻`、`SO₄²⁻`…），靠别名穷举不可行。归一化是从根上解决，不写死别名。

## 未验证的部分

- **UI 的实际观感**：自动化环境没有浏览器渲染能力。已自动验证的是：JS 语法、检索逻辑、主题与 CSS 变量的一一对应、无未定义变量引用。**但"好不好看"只能你自己看**
- 六套配色的对比度（比如亮色文字在选中行上是否够清晰）未做无障碍校验
- 原生的热键注册、文本注入、剪贴板策略：代码还没写（被工具链阻塞）
- 打包体积、常驻内存：未知

## 下一步

1. 你打开 `ui/index.html` 看一眼观感，告诉我要改什么（配色、行高、宽度、信息密度）
2. 定原生层走 A（.NET）还是 B（Tauri）——这个决定之后我才能写热键和注入
3. 注入层写完后，直接用 PRD 附录 B.2 的 7 个码点用例做回归