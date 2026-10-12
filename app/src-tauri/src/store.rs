//! 词条存储（SQLite）
//!
//! 为什么要换掉前端硬编码的 `entries.js`：
//! 1. Tauri 把 `ui/` 嵌进二进制，改一个词条要重新编译 —— 用户根本没法自己加词；
//! 2. 「用户的词条」和「官方词库包」必须能分开管理、分别升级。
//!
//! 内置包在编译期嵌入（`include_str!`），首次运行导入数据库；
//! 之后按包版本判断是否需要重导，**不会覆盖用户自己的词条**（`source='user'`）。

use crate::log::diag;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const BUILTIN_PACKS: &[&str] = &[
    include_str!("../../../packs/chemistry.zh.json"),
    include_str!("../../../packs/language.zh.json"),
];

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS packs (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  version       TEXT NOT NULL,
  source        TEXT NOT NULL,
  locale        TEXT,
  author        TEXT,
  license       TEXT,
  description   TEXT,
  installed_at  INTEGER NOT NULL,
  enabled       INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS entries (
  id          TEXT PRIMARY KEY,
  pack_id     TEXT,
  text        TEXT NOT NULL,
  kind        TEXT NOT NULL DEFAULT 'term',
  group_name  TEXT,
  note        TEXT,
  trigger     TEXT UNIQUE,
  format      TEXT NOT NULL DEFAULT 'plain',
  pinned      INTEGER NOT NULL DEFAULT 0,
  origin      TEXT NOT NULL DEFAULT 'pack',
  enabled     INTEGER NOT NULL DEFAULT 1,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS aliases (
  entry_id  TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  alias     TEXT NOT NULL,
  kind      TEXT NOT NULL DEFAULT 'text'
);
CREATE INDEX IF NOT EXISTS idx_aliases_alias ON aliases(alias);
CREATE TABLE IF NOT EXISTS settings (
  key    TEXT PRIMARY KEY,
  value  TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS usage (
  entry_id     TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
  use_count    INTEGER NOT NULL DEFAULT 0,
  last_used_at INTEGER
);
";

#[derive(Serialize, Deserialize)]
struct PackFile {
    pack: PackMeta,
    entries: Vec<PackEntry>,
}

#[derive(Serialize, Deserialize)]
struct PackMeta {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PackEntry {
    id: String,
    text: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    pinyin: Option<String>,
    #[serde(default)]
    initialism: Option<String>,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    format: Option<String>,
}

/// 交给前端的形状 —— 字段名必须与 `ui/search-core.js` 期待的一致（camelCase）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryOut {
    pub id: String,
    pub text: String,
    pub kind: String,
    pub group: String,
    pub pack: String,
    pub pack_name: String,
    pub aliases: Vec<String>,
    pub pinyin: String,
    pub initialism: String,
    pub note: String,
    pub trigger: String,
    pub origin: String,
    pub enabled: i64,
    pub use_count: i64,
    pub pinned: i64,
}

pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("shizi.db")
}

pub fn open(data_dir: &Path) -> rusqlite::Result<Connection> {
    std::fs::create_dir_all(data_dir).ok();
    let conn = Connection::open(db_path(data_dir))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 把编译期内置的包同步进数据库。
///
/// 只在「库里没有该包」或「版本号变了」时才重导。
/// 重导只删 `origin='pack'` 的条目，用户自己的词条（`origin='user'`）不受影响。
pub fn sync_builtin_packs(conn: &mut Connection) -> rusqlite::Result<usize> {
    let mut total = 0;
    for raw in BUILTIN_PACKS {
        let pf: PackFile = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(e) => {
                diag(&format!("内置包解析失败，跳过: {e}"));
                continue;
            }
        };
        let cur: Option<String> = conn
            .query_row(
                "SELECT version FROM packs WHERE id = ?1",
                params![pf.pack.id],
                |r| r.get(0),
            )
            .optional()?;
        if cur.as_deref() == Some(pf.pack.version.as_str()) {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM entries WHERE pack_id = ?1 AND origin = 'pack'",
            params![pf.pack.id],
        )?;
        tx.execute(
            "INSERT INTO packs (id, name, version, source, locale, author, license, description, installed_at)
             VALUES (?1, ?2, ?3, 'builtin', ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, version=excluded.version,
               locale=excluded.locale, author=excluded.author, license=excluded.license,
               description=excluded.description",
            params![
                pf.pack.id,
                pf.pack.name,
                pf.pack.version,
                pf.pack.locale,
                pf.pack.author,
                pf.pack.license,
                pf.pack.description,
                now_ts()
            ],
        )?;

        let ts = now_ts();
        for e in &pf.entries {
            tx.execute(
                "INSERT OR REPLACE INTO entries
                   (id, pack_id, text, kind, group_name, note, trigger, format, origin, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pack', 1, ?9, ?9)",
                params![
                    e.id,
                    pf.pack.id,
                    e.text,
                    e.kind.clone().unwrap_or_else(|| "term".into()),
                    e.group,
                    e.note,
                    e.trigger,
                    e.format.clone().unwrap_or_else(|| "plain".into()),
                    ts
                ],
            )?;
            tx.execute("DELETE FROM aliases WHERE entry_id = ?1", params![e.id])?;
            for a in &e.aliases {
                tx.execute(
                    "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'text')",
                    params![e.id, a],
                )?;
            }
            if let Some(py) = e.pinyin.as_ref().filter(|s| !s.is_empty()) {
                tx.execute(
                    "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'pinyin')",
                    params![e.id, py],
                )?;
            }
            if let Some(ini) = e.initialism.as_ref().filter(|s| !s.is_empty()) {
                tx.execute(
                    "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'initialism')",
                    params![e.id, ini],
                )?;
            }
        }
        tx.commit()?;
        diag(&format!(
            "已导入内置包 {} v{}（{} 条）",
            pf.pack.id,
            pf.pack.version,
            pf.entries.len()
        ));
        total += pf.entries.len();
    }
    Ok(total)
}

pub fn count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM entries WHERE enabled = 1", [], |r| {
        r.get(0)
    })
}

/// `include_disabled`：管理界面要看到并重新启用被停用的词条，所以要带上它们。
pub fn load_all(conn: &Connection, include_disabled: bool) -> rusqlite::Result<Vec<EntryOut>> {
    // 别名一次查完再分组，避免 N+1
    let mut by_entry: HashMap<String, (Vec<String>, String, String)> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT entry_id, alias, kind FROM aliases")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (eid, alias, kind) = row?;
            let slot = by_entry.entry(eid).or_default();
            match kind.as_str() {
                "pinyin" => slot.1 = alias,
                "initialism" => slot.2 = alias,
                _ => slot.0.push(alias),
            }
        }
    }

    let mut stmt = conn.prepare(
        "SELECT e.id, e.text, e.kind, COALESCE(e.group_name,''), COALESCE(e.pack_id,''),
                COALESCE(p.name,''), COALESCE(e.trigger,''), COALESCE(e.note,''),
                e.origin, e.enabled, e.pinned, COALESCE(u.use_count, 0)
           FROM entries e
           LEFT JOIN packs p ON p.id = e.pack_id
           LEFT JOIN usage u ON u.entry_id = e.id
          WHERE (?1 = 1 OR e.enabled = 1)
          ORDER BY e.id",
    )?;
    let flag: i64 = if include_disabled { 1 } else { 0 };
    let rows = stmt.query_map(params![flag], |r| {
        let id: String = r.get(0)?;
        let (aliases, pinyin, initialism) = by_entry.get(&id).cloned().unwrap_or_default();
        Ok(EntryOut {
            id,
            text: r.get(1)?,
            kind: r.get(2)?,
            group: r.get(3)?,
            pack: r.get(4)?,
            pack_name: r.get(5)?,
            aliases,
            pinyin,
            initialism,
            trigger: r.get(6)?,
            note: r.get(7)?,
            origin: r.get(8)?,
            enabled: r.get(9)?,
            pinned: r.get(10)?,
            use_count: r.get(11)?,
        })
    })?;
    rows.collect()
}

/// 触发码是否已被**别的**词条占用。返回占用者的文本，用于报错时告诉用户被谁占了。
pub fn trigger_owner(
    conn: &Connection,
    trigger: &str,
    exclude_id: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    let mut stmt =
        conn.prepare("SELECT id, text FROM entries WHERE trigger = ?1 AND enabled = 1")?;
    let mut rows = stmt.query(params![trigger])?;
    while let Some(r) = rows.next()? {
        let id: String = r.get(0)?;
        if Some(id.as_str()) == exclude_id {
            continue;
        }
        return Ok(Some(r.get(1)?));
    }
    Ok(None)
}

/// 生成一个不会撞的用户词条 id。
fn next_user_id(conn: &Connection) -> rusqlite::Result<String> {
    let base = format!("user.{}", now_ts());
    let mut cand = base.clone();
    let mut n = 1;
    while conn
        .query_row("SELECT 1 FROM entries WHERE id = ?1", params![cand], |_| Ok(()))
        .optional()?
        .is_some()
    {
        n += 1;
        cand = format!("{base}.{n}");
    }
    Ok(cand)
}

/// 新建或更新一条**用户自己的**词条（`origin='user'`）。
pub fn upsert_user_entry(
    conn: &Connection,
    id: Option<&str>,
    text: &str,
    trigger: Option<&str>,
    group: Option<&str>,
    note: Option<&str>,
    aliases: &[String],
) -> rusqlite::Result<String> {
    let eid = match id {
        Some(i) => i.to_string(),
        None => next_user_id(conn)?,
    };
    let ts = now_ts();
    conn.execute(
        "INSERT INTO entries
           (id, pack_id, text, kind, group_name, note, trigger, format, origin, enabled, created_at, updated_at)
         VALUES (?1, NULL, ?2, 'term', ?3, ?4, ?5, 'plain', 'user', 1, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
           text = excluded.text, group_name = excluded.group_name,
           note = excluded.note, trigger = excluded.trigger, updated_at = excluded.updated_at",
        params![eid, text, group, note, trigger, ts],
    )?;
    conn.execute("DELETE FROM aliases WHERE entry_id = ?1", params![eid])?;
    for a in aliases.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
        conn.execute(
            "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'text')",
            params![eid, a],
        )?;
    }
    Ok(eid)
}

pub fn delete_user_entry(conn: &Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM entries WHERE id = ?1 AND origin = 'user'",
        params![id],
    )
}

pub fn set_entry_enabled(conn: &Connection, id: &str, on: bool) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE entries SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, if on { 1 } else { 0 }, now_ts()],
    )
}

pub fn origin_of(conn: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT origin FROM entries WHERE id = ?1", params![id], |r| {
        r.get(0)
    })
    .optional()
}

/// 触发码 → 替换文本。供打字自动展开使用。
pub fn trigger_map(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT trigger, text FROM entries
          WHERE enabled = 1 AND trigger IS NOT NULL AND trigger <> ''",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut m = HashMap::new();
    for row in rows {
        let (t, txt) = row?;
        m.insert(t, txt);
    }
    Ok(m)
}

/// 开发/诊断用：把库里加载到的条数写进日志。
pub fn summary(conn: &Connection) -> String {
    match count(conn) {
        Ok(n) => {
            let packs: i64 = conn
                .query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))
                .unwrap_or(0);
            format!("{n} 条启用词条，{packs} 个包")
        }
        Err(e) => format!("读取失败: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn parse_all() -> Vec<PackFile> {
        BUILTIN_PACKS
            .iter()
            .map(|r| serde_json::from_str(r).expect("内置包 JSON 解析失败"))
            .collect()
    }

    /// 用户词条的完整生命周期：新增 → 读出 → 冲突检测 → 删除 → 停用。
    /// 在内存 SQLite 上跑，不碰用户真实数据，而且每次都是干净状态。
    #[test]
    fn user_entry_lifecycle() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        sync_builtin_packs(&mut conn).unwrap();
        let base = count(&conn).unwrap();
        assert!(base >= 100, "内置包应先导入，实际 {base}");

        // 新增一条自己的词条
        let id = upsert_user_entry(
            &conn,
            None,
            "测试组合串",
            Some(";test"),
            Some("测试分组"),
            Some("备注"),
            &["ceshi".to_string(), "tc".to_string()],
        )
        .unwrap();
        assert!(id.starts_with("user."), "id 应有 user. 前缀，实际 {id}");

        let all = load_all(&conn, true).unwrap();
        let e = all.iter().find(|x| x.id == id).expect("应能读回新词条");
        assert_eq!(e.text, "测试组合串");
        assert_eq!(e.trigger, ";test");
        assert_eq!(e.origin, "user");
        assert_eq!(e.aliases, vec!["ceshi".to_string(), "tc".to_string()]);
        assert_eq!(count(&conn).unwrap(), base + 1);

        // 冲突检测：自己不算冲突，别人算
        assert!(trigger_owner(&conn, ";test", Some(&id)).unwrap().is_none());
        assert_eq!(
            trigger_owner(&conn, ";test", None).unwrap().as_deref(),
            Some("测试组合串")
        );
        // 官方包已占的触发码也要能被发现（;mu 属于化学包）
        assert!(trigger_owner(&conn, ";mu", None).unwrap().is_some());

        // 触发码表要包含用户词条 —— 打字展开才认得它
        let map = trigger_map(&conn).unwrap();
        assert_eq!(map.get(";test").map(|s| s.as_str()), Some("测试组合串"));

        // 官方包的条目不许删
        assert_eq!(delete_user_entry(&conn, "chemistry.zh.0001").unwrap(), 0);

        // 停用 / 启用
        set_entry_enabled(&conn, "chemistry.zh.0001", false).unwrap();
        assert!(!load_all(&conn, false).unwrap().iter().any(|x| x.id == "chemistry.zh.0001"));
        assert!(load_all(&conn, true).unwrap().iter().any(|x| x.id == "chemistry.zh.0001"));
        set_entry_enabled(&conn, "chemistry.zh.0001", true).unwrap();
        assert!(load_all(&conn, false).unwrap().iter().any(|x| x.id == "chemistry.zh.0001"));

        // 删除自己的词条
        assert_eq!(delete_user_entry(&conn, &id).unwrap(), 1);
        assert_eq!(count(&conn).unwrap(), base);
        assert!(origin_of(&conn, &id).unwrap().is_none());
    }

    /// 导出 → 导入 往返。顺带覆盖三件事：冲突的触发码只清码不丢词、
    /// 重复导入是「更新」而不是新增、官方包 id 不允许被覆盖。
    #[test]
    fn export_and_import_round_trip() {
        let dir = std::env::temp_dir().join("shizi-pack-test");
        std::fs::create_dir_all(&dir).unwrap();

        // 源库：内置包 + 一条自己的词条
        let mut src = Connection::open_in_memory().unwrap();
        src.execute_batch(SCHEMA).unwrap();
        sync_builtin_packs(&mut src).unwrap();
        upsert_user_entry(
            &src,
            None,
            "测试组合",
            Some(";tst"),
            Some("测试分组"),
            None,
            &["cs".to_string()],
        )
        .unwrap();

        let (fname, text) = export_pack(&src, None).unwrap();
        assert!(fname.starts_with("mine-"), "导出文件名: {fname}");
        let path = dir.join("mine.json");
        std::fs::write(&path, &text).unwrap();
        let p = path.to_str().unwrap().to_string();
        // 导出的文件必须能被自己的解析器读回（格式对称）
        assert!(read_pack_file(&p).is_ok());

        // 目标库要像真实用户的库一样：先有内置包，再有一条占住 ;tst 的词条。
        // （第一次写这个测试时我漏了 sync_builtin_packs，导致"官方包已存在"这个
        //   前置条件不成立，后面那条断言才挂掉。）
        let mut dst = Connection::open_in_memory().unwrap();
        dst.execute_batch(SCHEMA).unwrap();
        sync_builtin_packs(&mut dst).unwrap();
        upsert_user_entry(&dst, None, "占位词条", Some(";tst"), None, None, &[]).unwrap();

        let pv = preview_import(&dst, &p).unwrap();
        assert_eq!(pv.total, 1);
        assert_eq!(pv.trigger_conflicts, 1, "应发现 1 个触发码冲突: {pv:?}");
        assert!(!pv.replacing);

        let r = apply_import(&mut dst, &p).unwrap();
        assert_eq!(r.imported, 1);
        assert_eq!(r.trigger_dropped, 1);

        let all = load_all(&dst, true).unwrap();
        let got = all.iter().find(|x| x.text == "测试组合").expect("词条应已导入");
        assert_eq!(got.origin, "imported");
        assert_eq!(got.trigger, "", "撞车的触发码应被清空，但词条要留下");
        assert_eq!(got.aliases, vec!["cs".to_string()]);
        assert_eq!(got.group, "测试分组");

        // 再导一次：是更新，不是新增
        assert!(preview_import(&dst, &p).unwrap().replacing);
        apply_import(&mut dst, &p).unwrap();
        assert_eq!(
            load_all(&dst, true)
                .unwrap()
                .iter()
                .filter(|x| x.text == "测试组合")
                .count(),
            1
        );

        // 官方包 id 不允许被覆盖
        let (_, chem) = export_pack(&src, Some("chemistry.zh")).unwrap();
        let cp = dir.join("chem.json");
        std::fs::write(&cp, &chem).unwrap();
        let cps = cp.to_str().unwrap().to_string();
        assert!(preview_import(&dst, &cps).is_err(), "官方包 id 应拒绝覆盖");
        assert!(apply_import(&mut dst, &cps).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtin_packs_parse_and_ids_are_unique() {
        let packs = parse_all();
        // 2026-10-12：育种包已移出，内置包从 3 个变 2 个（化学 / 语言）
        assert!(packs.len() >= 2, "内置包少于 2 个");
        let mut ids = HashSet::new();
        for p in &packs {
            for e in &p.entries {
                assert!(ids.insert(e.id.clone()), "条目 id 重复: {}", e.id);
            }
        }
        assert!(ids.len() >= 100, "词条总数异常: {}", ids.len());
    }

    /// 这条测试是为了拦住一个**真实踩过的坑**：
    /// 触发码曾经只在 Python 生成脚本里算、只写进前端的 `entries.js`，
    /// 而数据库是直接从 `packs/` 导入的 —— 结果库里只有 4 个触发码，
    /// 面板几乎不显示触发码，展开功能也无从匹配。
    #[test]
    fn builtin_packs_carry_their_triggers() {
        let packs = parse_all();
        let mut all = HashSet::new();
        let mut n = 0usize;
        for p in &packs {
            for e in &p.entries {
                if let Some(t) = e.trigger.as_ref().filter(|t| !t.is_empty()) {
                    n += 1;
                    assert!(t.starts_with(';'), "触发码必须以分号开头: {t} ({})", e.id);
                    assert!(t.is_ascii(), "触发码必须是纯 ASCII: {t} ({})", e.id);
                    assert!(all.insert(t.clone()), "触发码跨包重复: {t}");
                }
            }
        }
        assert!(
            n >= 100,
            "触发码只有 {n} 个 —— packs 里多半没写回触发码，先跑 app/tools/normalize_packs.py"
        );
        for want in [";mu", ";sigma", ";13c", ";cheng", ";xi"] {
            assert!(all.contains(want), "缺关键触发码: {want}");
        }
    }
}

// ============================ 设置 ============================

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| {
        r.get(0)
    })
    .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ============================ 包导入导出 ============================

/// 导出用的包 id：用户自己的词条统一装进一个叫 `mine` 的包，
/// 这样重复导入时是「更新」而不是堆出无数个包。
pub const USER_PACK_ID: &str = "mine";

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub pack_id: String,
    pub pack_name: String,
    pub version: String,
    pub total: usize,
    /// 触发码已被现有词条占用、导入后将被清空的条数
    pub trigger_conflicts: usize,
    /// 冲突明细（触发码 + 占用者内容），最多列 10 条
    pub conflicts: Vec<String>,
    /// 目标包已存在（将会是更新而非新增）
    pub replacing: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub pack_id: String,
    pub imported: usize,
    /// 因为没有触发码而只能用搜索的条数
    pub trigger_dropped: usize,
    pub dropped_detail: Vec<String>,
}

fn read_pack_file(path: &str) -> Result<PackFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读不到文件: {e}"))?;
    let pf: PackFile =
        serde_json::from_str(&text).map_err(|e| format!("不是合法的词库包: {e}"))?;
    if pf.entries.is_empty() {
        return Err("这个包里没有词条".into());
    }
    Ok(pf)
}

/// 把某个包（或用户自己的词条）导出成词库包 JSON 文本。
pub fn export_pack(conn: &Connection, pack_id: Option<&str>) -> Result<(String, String), String> {
    let (meta, entries): (PackMeta, Vec<PackEntry>) = match pack_id {
        // 导出用户自己的词条
        None => {
            let rows = load_all(conn, false).map_err(|e| e.to_string())?;
            let mine: Vec<EntryOut> = rows.into_iter().filter(|e| e.origin == "user").collect();
            if mine.is_empty() {
                return Err("还没有自己的词条可以导出".into());
            }
            let entries = mine.iter().map(entry_to_pack).collect::<Vec<_>>();
            (
                PackMeta {
                    id: USER_PACK_ID.to_string(),
                    name: "我的词条".into(),
                    version: now_ts().to_string(),
                    locale: Some("zh-CN".into()),
                    author: None,
                    license: None,
                    description: Some("由拾字导出的用户词条".into()),
                },
                entries,
            )
        }
        Some(pid) => {
            let meta = conn
                .query_row(
                    "SELECT id, name, version, locale, author, license, description
                       FROM packs WHERE id = ?1",
                    params![pid],
                    |r| {
                        Ok(PackMeta {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            version: r.get(2)?,
                            locale: r.get::<_, Option<String>>(3)?,
                            author: r.get::<_, Option<String>>(4)?,
                            license: r.get::<_, Option<String>>(5)?,
                            description: r.get::<_, Option<String>>(6)?,
                        })
                    },
                )
                .map_err(|_| format!("找不到包 {pid}"))?;
            let rows = load_all(conn, false).map_err(|e| e.to_string())?;
            let entries = rows
                .iter()
                .filter(|e| e.pack == pid)
                .map(entry_to_pack)
                .collect::<Vec<_>>();
            (meta, entries)
        }
    };

    let file = PackFile { pack: meta, entries };
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    let filename = format!("{}-{}.json", file.pack.id, now_ts());
    Ok((filename, text))
}

/// 库内条目 → 包格式条目。空的字段不写进去，导出的文件才干净。
fn entry_to_pack(e: &EntryOut) -> PackEntry {
    PackEntry {
        id: e.id.clone(),
        text: e.text.clone(),
        kind: Some(e.kind.clone()),
        group: Some(e.group.clone()).filter(|g| !g.is_empty()),
        aliases: e.aliases.clone(),
        pinyin: Some(e.pinyin.clone()).filter(|s| !s.is_empty()),
        initialism: Some(e.initialism.clone()).filter(|s| !s.is_empty()),
        trigger: Some(e.trigger.clone()).filter(|t| !t.is_empty()),
        note: Some(e.note.clone()).filter(|n| !n.is_empty()),
        format: Some("plain".into()),
    }
}

/// 导入预览：先让用户看清楚会带进来什么、有多少触发码会撞车。
pub fn preview_import(conn: &Connection, path: &str) -> Result<ImportPreview, String> {
    let pf = read_pack_file(path)?;
    let mut conflicts = Vec::new();
    let mut n = 0usize;
    for e in &pf.entries {
        if let Some(t) = e.trigger.as_deref().filter(|t| !t.is_empty()) {
            if let Some(owner) = trigger_owner(conn, t, None).map_err(|err| err.to_string())? {
                n += 1;
                if conflicts.len() < 10 {
                    conflicts.push(format!("{t} 已被「{}」占用", &owner[..owner.len().min(20)]));
                }
            }
        }
    }
    // 注意：包表里叫 source（builtin/imported），条目表里才叫 origin（pack/user/imported）——
    // 这两个名字很容易混，测试里已经因为混用挂过一次。
    let replacing = conn
        .query_row(
            "SELECT source FROM packs WHERE id = ?1",
            params![pf.pack.id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if replacing.as_deref() == Some("builtin") {
        return Err(format!("{} 是官方词库包的 id，不能覆盖", pf.pack.id));
    }
    Ok(ImportPreview {
        pack_id: pf.pack.id.clone(),
        pack_name: pf.pack.name.clone(),
        version: pf.pack.version.clone(),
        total: pf.entries.len(),
        trigger_conflicts: n,
        conflicts,
        replacing: replacing.is_some(),
    })
}

/// 真正导入。撞车的触发码**不丢词条**——只把它的触发码清空，让它退化成「只能搜索」。
pub fn apply_import(conn: &mut Connection, path: &str) -> Result<ImportResult, String> {
    let pf = read_pack_file(path)?;
    if let Some(origin) = conn
        .query_row(
            "SELECT source FROM packs WHERE id = ?1",
            params![pf.pack.id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        if origin == "builtin" {
            return Err(format!("{} 是官方词库包的 id，不能覆盖", pf.pack.id));
        }
    }

    let ts = now_ts();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM entries WHERE pack_id = ?1 AND origin = 'imported'",
        params![pf.pack.id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO packs (id, name, version, source, locale, author, license, description, installed_at)
         VALUES (?1, ?2, ?3, 'imported', ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, version=excluded.version,
           locale=excluded.locale, author=excluded.author, license=excluded.license,
           description=excluded.description",
        params![
            pf.pack.id, pf.pack.name, pf.pack.version,
            pf.pack.locale, pf.pack.author, pf.pack.license, pf.pack.description, ts
        ],
    )
    .map_err(|e| e.to_string())?;

    let mut imported = 0usize;
    let mut dropped = Vec::new();
    for e in &pf.entries {
        // 触发码撞车就清掉，词条照样进来
        let trigger = match e.trigger.as_deref().filter(|t| !t.is_empty()) {
            Some(t) => match trigger_owner(&tx, t, None).map_err(|err| err.to_string())? {
                Some(owner) => {
                    dropped.push(format!("{t}（{} 已占用）", &owner[..owner.len().min(16)]));
                    None
                }
                None => Some(t.to_string()),
            },
            None => None,
        };
        // 原 id 若已带本包前缀就先剥掉，免得叠成 chemistry.zh.chemistry.zh.0001
        let raw = e
            .id
            .strip_prefix(&format!("{}.", pf.pack.id))
            .unwrap_or(&e.id);
        let new_id = format!("{}.{}", pf.pack.id, raw);
        tx.execute(
            "INSERT OR REPLACE INTO entries
               (id, pack_id, text, kind, group_name, note, trigger, format, origin, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'imported', 1, ?9, ?9)",
            params![
                new_id, pf.pack.id, e.text,
                e.kind.clone().unwrap_or_else(|| "term".into()),
                e.group, e.note, trigger,
                e.format.clone().unwrap_or_else(|| "plain".into()), ts
            ],
        )
        .map_err(|err| err.to_string())?;
        for a in &e.aliases {
            tx.execute(
                "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'text')",
                params![new_id, a],
            )
            .map_err(|err| err.to_string())?;
        }
        if let Some(py) = e.pinyin.as_deref().filter(|s| !s.is_empty()) {
            tx.execute(
                "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'pinyin')",
                params![new_id, py],
            )
            .map_err(|err| err.to_string())?;
        }
        if let Some(ini) = e.initialism.as_deref().filter(|s| !s.is_empty()) {
            tx.execute(
                "INSERT INTO aliases (entry_id, alias, kind) VALUES (?1, ?2, 'initialism')",
                params![new_id, ini],
            )
            .map_err(|err| err.to_string())?;
        }
        imported += 1;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(ImportResult {
        pack_id: pf.pack.id.clone(),
        imported,
        trigger_dropped: dropped.len(),
        dropped_detail: dropped.into_iter().take(10).collect(),
    })
}

/// 移除一个导入的包（官方包不允许）。
pub fn remove_pack(conn: &Connection, pack_id: &str) -> rusqlite::Result<usize> {
    // 只移除导入的包；官方包留在库里，用户自己的词条也不受影响
    let n = conn.execute(
        "DELETE FROM entries WHERE pack_id = ?1 AND origin = 'imported'",
        params![pack_id],
    )?;
    conn.execute(
        "DELETE FROM packs WHERE id = ?1 AND source = 'imported'",
        params![pack_id],
    )?;
    Ok(n)
}
