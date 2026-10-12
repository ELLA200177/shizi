# 体积与内存实测基线

> 测于 2026-10-12。环境：Windows 11 (26100)、WebView2 154.0.4258.62、release 构建。
> 拾字为单实例运行（面板 + 管理窗口都已加载，均处于隐藏状态）。

这份文档记的是**可复现的实测值**，不是估算。它替换掉 PRD 里早期凭印象写下的性能假设。

---

## 0. 方法（先说口径，否则数字没有意义）

**内存必须用「私有工作集」，不能用「工作集」。**

第一次测量我用 `Win32_Process.WorkingSetSize`，得到拾字 510 MB、Espanso 57 MB。
这个口径是错的：**工作集会把共享 DLL 的页在每个进程里各算一遍**。
拾字有 7 个 WebView2 进程共享同一批运行时 DLL，于是同一块内存被算了 7 次。

换成 `Win32_PerfFormattedData_PerfProc_Process.WorkingSetPrivate`（私有工作集，只算这个进程独占的页）：

| 口径 | 拾字 | Espanso | 倍数 |
|---|---|---|---|
| 工作集（含共享，**口径错误**） | 509 MB | 57 MB | 9× |
| **私有工作集（正确）** | **162 MB** | **7.2 MB** | **22×** |

数字相差 3 倍多。**记录性能数据时，口径比数字本身更重要。**

---

## 1. 体积

| 项 | 实测 |
|---|---|
| 拾字 release 单文件 | **7.7 MB** |
| 拾字 debug 单文件 | 15.2 MB |
| 拾字 SQLite 词库 | 96 KB |
| Espanso 安装目录 | 23.3 MB |

拾字在**体积上确实赢了**（约 1/3），这正是选 Tauri 而非 Electron 的原因：
它不打包浏览器，直接用系统的 WebView2 运行时。

---

## 2. 内存逐进程拆解（拾字，release）

| 私有内存 | 进程 | 每个窗口都要付吗 |
|---|---|---|
| 4.3 MB | 拾字主程序（Rust） | — |
| 36.0 MB | WebView2 浏览器进程 | ✅ 只要有一个窗口就付 |
| **59.7 MB** | **WebView2 GPU 进程** | ✅ 同上 |
| 6.6 MB | WebView2 工具进程（网络） | ✅ 同上 |
| 3.1 MB | WebView2 工具进程（存储） | ✅ 同上 |
| 1.8 MB | WebView2 崩溃处理 | ✅ 同上 |
| **27.2 MB** | 渲染进程（**搜索面板**） | 每个窗口一个 |
| **22.2 MB** | 渲染进程（**词库管理窗口**） | 每个窗口一个 |
| **合计 161 MB** | 8 个进程 | |

**两条关键结论：**

1. **固定底噪约 107 MB**（浏览器 36 + GPU 59.7 + 工具/崩溃 11.5）。
   这部分与"开了几个窗口"无关，**只要拾字在运行就躲不掉**。
2. 每个额外窗口约 22–27 MB。

---

## 3. 与 Espanso 的对比

| 指标 | 拾字 | Espanso | 谁优 |
|---|---|---|---|
| 磁盘体积 | **7.7 MB** | 23.3 MB | 🟢 拾字 |
| **内存（私有工作集）** | **162 MB** | **7.2 MB** | 🔴 **拾字 22× 更差** |
| 进程数 | 8 | 3 | 🟢 Espanso |
| 支持平台 | 仅 Windows | 三平台 | 🟢 Espanso |

**这个差距是技术栈决定的，不是实现质量。**

Espanso 用 wxWidgets 原生控件：控件由系统绘制，内存就是那 7 MB。
拾字用 WebView2：等于在进程里带了一个浏览器引擎，107 MB 是它的入场费。

所以：**拾字用 154 倍的内存，换来了 3 倍的体积优势和自绘 UI 的自由度。**
这笔交易对错取决于目标用户——桌面工具类产品里，"常驻 162 MB"不算灾难
（浏览器一个标签页常 100 MB+），但确实不小。

---

## 4. 由此需要修正的 PRD 假设

PRD 第 8 节原本写着：

> 常驻内存 **< 80MB** —— Tauri 相对 Electron 的核心优势，必须守住

**这条不成立，且理由写错了。** 实测 162 MB，是目标的两倍。

- 「Tauri 内存低」这个说法**在本项目不成立**。Tauri 相对 Electron 的真实优势是**体积**
  （不打包浏览器），内存上两者同属"带浏览器引擎"这一档。
- 80 MB 这个数字是我早期凭印象定的，没有依据，**应当重新设定**。

---

## 5. 如果要降内存：三个选项与代价

| 选项 | 预期收益 | 代价 | 建议 |
|---|---|---|---|
| **管理窗口懒创建**（首次打开才建） | 省 ~22 MB（14%） | 首次打开有几百毫秒延迟（用户会以为"点了没反应"）；窗口参数从配置挪进代码，**更容易漏**（本项目已因漏 `focus: false` 犯过一次"启动抢前台"） | ❌ 性价比低，不做 |
| 给 WebView2 传 `--disable-gpu` 等启动参数 | 可能压掉 GPU 那 59.7 MB 的一部分 | 牺牲渲染流畅度；实际效果未测 | ⚠️ 可以先试一下再定 |
| **不用 WebView2，改原生控件** | 拿掉 107 MB 底噪 | UI 全部重写，是另一个量级的工程 | 只在"内存是硬指标"时才考虑 |

**结论：当前不做内存优化。** 收益上限只有 14%，而动 GPU 底噪要换技术栈。
把精力放在功能完整度和公开分发的门槛上更划算。

---

## 6. 复现方法

```powershell
# 体积
Get-Item app/src-tauri/target/release/shizi.exe | Select-Object Length

# 内存（私有工作集，按进程树求和）
$all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
$roots = @($all | Where-Object Name -eq 'shizi.exe' | Select-Object -ExpandProperty ProcessId)
$seen = New-Object 'System.Collections.Generic.HashSet[int]'
$q = New-Object 'System.Collections.Generic.Queue[int]'
foreach ($r in $roots) { [void]$q.Enqueue($r) }
while ($q.Count -gt 0) {
  $cur = $q.Dequeue(); if (-not $seen.Add($cur)) { continue }
  foreach ($p in $all) { if ($p.ParentProcessId -eq $cur) { [void]$q.Enqueue($p.ProcessId) } }
}
$perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process
($perf | Where-Object { $seen.Contains([int]$_.IDProcess) } |
  Measure-Object WorkingSetPrivate -Sum).Sum / 1MB
```

看单个进程是干什么的：

```powershell
Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
  Select-Object ProcessId, CommandLine
# CommandLine 里的 --type=renderer / gpu-process / utility 就是它的角色
```

---

## 7. 尚未测量

| 项 | 为什么没测 |
|---|---|
| 冷启动时间 | 需要精确计时脚本，且和磁盘缓存强相关，单次值意义不大 |
| 面板唤起延迟 | 同上（PRD 目标是 <80ms，尚未验证） |
| 注入延迟 | 同上（PRD 目标是 <150ms，尚未验证） |
| 安装包体积 | 需要 `cargo tauri build`（要装 tauri-cli），当前只有 exe |
| 高 DPI / 多显示器下的表现 | 需要人工在不同缩放比下看 |

> 注：这台机器是开发机，同时跑着其他程序，以上内存为单次采样。
> 数量级可信，精确到个位数的比较不必当真。
