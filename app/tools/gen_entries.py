# 从 packs/*.json 生成 app/ui/entries.js
#   python app/tools/gen_entries.py
#
# 这份产物**只服务于浏览器预览**（直接打开 index.html / gallery.html 时用）。
# 真实应用走 SQLite：内置包在编译期嵌入二进制，首次运行导入数据库。
#
# 触发码的唯一来源是 packs/*.json 本身 —— 需要重新计算时跑
#   python app/tools/normalize_packs.py
# 本脚本不做任何生成，只做字段搬运，避免出现两份实现。
import io, json, os, sys
sys.stdout.reconfigure(encoding="utf-8")

app_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # .../app
root = os.path.dirname(app_dir)                                       # 项目根
packs = ["chemistry.zh.json", "language.zh.json"]

all_entries = []
for f in packs:
    d = json.load(io.open(os.path.join(root, "packs", f), encoding="utf-8"))
    for it in d["entries"]:
        all_entries.append({
            "id": it["id"],
            "text": it["text"],
            "kind": it["kind"],
            "group": it.get("group", ""),
            "pack": d["pack"]["id"],
            "packName": d["pack"]["name"],
            "aliases": it.get("aliases", []),
            "pinyin": it.get("pinyin", ""),
            "initialism": it.get("initialism", ""),
            "note": it.get("note", ""),
            "trigger": it.get("trigger", ""),
            "useCount": 0,
            "pinned": 0,
        })

out = os.path.join(app_dir, "ui", "entries.js")
with io.open(out, "w", encoding="utf-8", newline="\n") as fh:
    fh.write("// 自动生成，请勿手改。来源: ../../packs/*.json\n")
    fh.write("// 重新生成: python app/tools/gen_entries.py\n")
    fh.write("window.SHIZI_ENTRIES = ")
    json.dump(all_entries, fh, ensure_ascii=False, indent=0, separators=(",", ":"))
    fh.write(";\n")

trigs = [e["trigger"] for e in all_entries if e["trigger"]]
print("已生成:", len(all_entries), "条")
print("带触发码:", len(trigs), "（唯一:", len(set(trigs)) == len(trigs), "）")
print("文件大小:", os.path.getsize(out), "字节")
