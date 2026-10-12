# 把触发码写回 packs/*.json —— 让包文件成为唯一权威数据
#   python app/tools/normalize_packs.py
#
# 为什么需要这一步：触发码原本只在 gen_entries.py 里生成、只写进 entries.js，
# 结果「数据库从 packs 导入」时拿到的触发码寥寥无几。两份数据源必然漂移。
# 现在改成：packs/*.json 里直接带上最终触发码，其余工具只做搬运。
#
# 规则（PRD 7.4）：
#   1. 包里已显式写 trigger 的，原样保留（用户的既有习惯优先）
#   2. 否则按 拼音首字母 -> 全拼 -> 无空格 ASCII 别名 依次尝试
#   3. 全局唯一；撞车就顺延到下一个候选
#   4. 候选若已是「别的词条」的别名，直接跳过（否则搜这个码会被那条别名抢走）
#   5. 都不行则留空（不写 trigger 字段）
import io, json, os, re, sys
sys.stdout.reconfigure(encoding="utf-8")

root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
packs = ["chemistry.zh.json", "language.zh.json"]


def candidates(e):
    out = []
    if e.get("initialism"):
        out.append(e["initialism"].lower())
    if e.get("pinyin"):
        out.append(e["pinyin"].replace(" ", "").lower())
    for a in e.get("aliases", []):
        if a.isascii() and " " not in a and len(a) >= 2:
            out.append(a.lower())
    seen, uniq = set(), []
    for c in out:
        if c not in seen:
            seen.add(c)
            uniq.append(c)
    return uniq


def norm(t):
    if not t:
        return ""
    t = t.strip()
    return t if t.startswith(";") else ";" + t


all_entries, raw_texts = [], {}
for f in packs:
    raw = io.open(os.path.join(root, "packs", f), encoding="utf-8").read()
    raw_texts[f] = raw
    d = json.loads(raw)
    for e in d["entries"]:
        e["_pack"] = f
        all_entries.append(e)

# 别名索引
alias_owner = {}
for e in all_entries:
    for a in e.get("aliases", []):
        alias_owner.setdefault(a.lower(), set()).add(e["id"])

used, report = {}, []
# 第一轮：显式
for e in all_entries:
    t = norm(e.get("trigger"))
    if t and t not in used:
        used[t] = e["id"]
        e["_final"] = t
    else:
        if t:
            report.append("  显式撞车被忽略: %s (%s)" % (t, e["id"]))
        e["_final"] = ""

# 第二轮：自动
for e in all_entries:
    if e["_final"]:
        continue
    for c in candidates(e):
        t = ";" + c
        if t in used:
            continue
        if alias_owner.get(c, set()) - {e["id"]}:
            continue
        used[t] = e["id"]
        e["_final"] = t
        break

# 写回：先删掉原有 trigger 字段，再按需插到 id 之后（保证幂等）
changed = 0
for f in packs:
    lines = raw_texts[f].split("\n")
    out = []
    for line in lines:
        m = re.match(r'^(\s*)\{"id":"([^"]+)"(.*)$', line)
        if not m:
            out.append(line)
            continue
        indent, eid, rest = m.group(1), m.group(2), m.group(3)
        ent = next((x for x in all_entries if x["id"] == eid), None)
        if ent is None:
            out.append(line)
            continue
        body = re.sub(r',"trigger":"[^"]*"', "", rest)
        t = ent["_final"]
        if t:
            body = ',"trigger":"' + t + '"' + body
        newline = indent + '{"id":"' + eid + '"' + body
        if newline != line:
            changed += 1
        out.append(newline)
    io.open(os.path.join(root, "packs", f), "w", encoding="utf-8", newline="\n").write("\n".join(out))

# 校验 + 版本号提升（让数据库的版本判断触发重导）
print("写回包文件，改动行数:", changed)
for f in packs:
    p = os.path.join(root, "packs", f)
    # 先读、校验，再写 —— 绝不能在写完前把文件读回来（会被截断）
    s = io.open(p, encoding="utf-8").read()
    d = json.loads(s)
    ids = [e["id"] for e in d["entries"]]
    assert len(ids) == len(set(ids)), f + " id 重复"
    trigs = [e["trigger"] for e in d["entries"] if e.get("trigger")]
    assert len(trigs) == len(set(trigs)), f + " 触发码重复"
    old_ver = d["pack"]["version"]
    d["pack"]["version"] = "0.2.0"
    s = re.sub(r'"version":\s*"[^"]*"', '"version": "0.2.0"', s, count=1)
    s = re.sub(r'"entry_count":\s*\d+', '"entry_count": %d' % len(d["entries"]), s, count=1)
    io.open(p, "w", encoding="utf-8", newline="\n").write(s)
    print("  %-18s %d 条, %d 个触发码, version %s -> 0.2.0" % (f, len(ids), len(trigs), old_ver))
print("有触发码的合计:", sum(1 for e in all_entries if e["_final"]))
for r in report:
    print(r)
