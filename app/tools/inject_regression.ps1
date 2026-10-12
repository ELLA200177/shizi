# 拾字 · 注入回归测试（用受控 WinForms 文本框接收 shizi.exe 的真实注入）
$ErrorActionPreference = 'Continue'
Write-Output ('HOST PSVersion=' + $PSVersionTable.PSVersion.ToString())

Add-Type -AssemblyName System.Windows.Forms -ErrorAction SilentlyContinue
if (-not ('System.Windows.Forms.Form' -as [type])) {
  [void][System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms')
}
if (-not ('System.Windows.Forms.Form' -as [type])) {
  [void][System.Reflection.Assembly]::Load('System.Windows.Forms, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089')
}
Write-Output ('WinForms loaded=' + [bool]('System.Windows.Forms.Form' -as [type]))
if (-not ('System.Windows.Forms.Form' -as [type])) { exit 99 }

# 相对脚本自身定位，避免把本机路径写进仓库
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $here '..\src-tauri\target\debug\shizi.exe'
$tmp = Join-Path $env:TEMP 'shizi_payload.txt'

$form = New-Object System.Windows.Forms.Form
$form.Text = 'inject regression target'
$form.Width = 720
$form.Height = 140
$form.StartPosition = 'CenterScreen'
$tb = New-Object System.Windows.Forms.TextBox
$tb.Multiline = $true
$tb.Font = New-Object System.Drawing.Font('Consolas', 13)
$tb.Dock = 'Fill'
$form.Controls.Add($tb) | Out-Null
$form.TopMost = $true
$form.Show() | Out-Null
$form.Activate() | Out-Null
$tb.Focus() | Out-Null
1..30 | ForEach-Object { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 25 }

$cases = @(
  @{ n = 'delta13C';       cp = @(0x03B4, 0x00B9, 0x00B3, 0x0043) },
  @{ n = 'astral U+20BB7'; cp = @(0xD842, 0xDFB7) },
  @{ n = 'micro U+00B5';   cp = @(0x00B5) },
  @{ n = 'celsius U+2103'; cp = @(0x2103) },
  @{ n = 'cjk 6 chars';    cp = @(0x809D, 0x8C46, 0x72B6, 0x6838, 0x53D8, 0x6027) },
  @{ n = 'emdash x2';      cp = @(0x2014, 0x2014) },
  @{ n = 'nbsp U+00A0';    cp = @(0x00A0) }
)

$fail = 0
foreach ($c in $cases) {
  $text = -join ($c.cp | ForEach-Object { [char]$_ })
  [System.IO.File]::WriteAllText($tmp, $text, (New-Object System.Text.UTF8Encoding($false)))
  $tb.Clear()
  $form.Activate() | Out-Null
  $tb.Focus() | Out-Null
  1..15 | ForEach-Object { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 20 }

  $proc = Start-Process $exe -ArgumentList @('--inject-file', $tmp) -PassThru -WindowStyle Hidden
  $deadline = (Get-Date).AddSeconds(8)
  while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    [System.Windows.Forms.Application]::DoEvents()
    Start-Sleep -Milliseconds 15
  }
  1..60 | ForEach-Object { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 20 }

  $got = $tb.Text
  $ok = ($got -ceq $text)
  if (-not $ok) { $fail++ }
  $gcp = ($got.ToCharArray() | ForEach-Object { 'U+{0:X4}' -f [int]$_ }) -join ' '
  if ($ok) {
    Write-Output ('OK   ' + $c.n.PadRight(16) + ' [' + $gcp + ']')
  } else {
    $ecp = ($c.cp | ForEach-Object { 'U+{0:X4}' -f $_ }) -join ' '
    Write-Output ('FAIL ' + $c.n.PadRight(16) + ' got=[' + $gcp + '] want=[' + $ecp + ']')
  }
}
$form.Close()
Write-Output ('RESULT cases=' + $cases.Count + ' failed=' + $fail)
exit $fail
