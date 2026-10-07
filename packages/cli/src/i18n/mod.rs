//! dx CLI 한국어화 코어: 언어 판정, 번역표 적재, 줄 단위 번역 훅.
//!
//! `DX_LANG=en`(대소문자 무관, `en` 으로 시작) 이면 어떤 훅도 번역표를 적재하지 않고 입력을 그대로 돌려준다.
//! 번역표는 `locales/ko.toml` 을 바이너리에 내장하며, 한국어 모드에서 처음 번역이 필요할 때 1회만 파싱한다.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Deserialize;

pub(crate) mod clap;
pub(crate) mod fmt;
mod matcher;
pub(crate) mod web;

use matcher::{
    MIN_LITERAL_ALPHA, MIN_OPEN_LITERAL_LEN, Pattern, Template, parse_ko_template, parse_rust_fmt,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Lang {
    Ko,
    En,
}

/// `DX_LANG` 값을 언어로 바꾼다. 없거나 비면 Ko, 소문자로 바꿔 `en` 으로 시작하면 En, 그 밖은 Ko.
fn parse_lang(v: Option<&str>) -> Lang {
    match v {
        None | Some("") => Lang::Ko,
        Some(s) if s.to_lowercase().starts_with("en") => Lang::En,
        Some(_) => Lang::Ko,
    }
}

/// 테스트에서는 항상 En(기존 테스트가 upstream 출력을 그대로 보게 한다).
#[cfg(test)]
pub(crate) fn lang() -> Lang {
    Lang::En
}

#[cfg(not(test))]
pub(crate) fn lang() -> Lang {
    static LANG: std::sync::OnceLock<Lang> = std::sync::OnceLock::new();
    *LANG.get_or_init(|| {
        parse_lang(
            std::env::var_os("DX_LANG")
                .map(|v| v.to_string_lossy().into_owned())
                .as_deref(),
        )
    })
}

pub(crate) fn is_ko() -> bool {
    lang() == Lang::Ko
}

static MACHINE_MODE: AtomicBool = AtomicBool::new(false);

/// 기계용 출력(JSON, print, completions 등)을 쓰는 실행임을 표시한다. 이후 모든 번역 훅은 입력을 그대로 돌려준다.
pub(crate) fn set_machine_mode() {
    MACHINE_MODE.store(true, Ordering::Relaxed);
}

/// 번역표를 실제로 파싱한 횟수(테스트 계측).
pub(crate) static LOADS: AtomicUsize = AtomicUsize::new(0);

const KO_SRC: &str = include_str!("../../locales/ko.toml");

/// 내장 번역표 원문(테스트가 전역 표를 적재하지 않고 직접 파싱할 때 쓴다).
#[cfg(test)]
pub(crate) fn ko_source() -> &'static str {
    KO_SRC
}

static KO_TABLE: LazyLock<Table> = LazyLock::new(|| {
    LOADS.fetch_add(1, Ordering::SeqCst);
    // 오류가 있는 항목은 건너뛰고, 파싱 자체가 실패하면 빈 표가 된다(패닉 금지).
    Table::parse(KO_SRC).0
});

/// 언어에 맞는 번역표. En 이면 None 이며 아무것도 적재하지 않는다.
fn table_for(lang: Lang) -> Option<&'static Table> {
    match lang {
        Lang::En => None,
        Lang::Ko => Some(&*KO_TABLE),
    }
}

/// 훅이 쓰는 번역표 판정(기계 모드면 None).
fn hook_table_for(lang: Lang, machine: bool) -> Option<&'static Table> {
    match lang {
        // En 은 번역표도 기계 모드도 보지 않는다.
        Lang::En => None,
        Lang::Ko if machine => None,
        Lang::Ko => table_for(Lang::Ko),
    }
}

#[cfg(test)]
thread_local! {
    /// 테스트 전용 주입 표. 설정되면 cfg(test) 의 훅이 전역 언어·KO_TABLE 대신 이것을 쓴다(LOADS 불변).
    static TEST_TABLE: std::cell::Cell<Option<&'static Table>> = const { std::cell::Cell::new(None) };
}

/// 테스트 전용: 현재 스레드의 주입 표를 설정/해제한다.
#[cfg(test)]
pub(crate) fn set_test_table(t: Option<&'static Table>) {
    TEST_TABLE.with(|c| c.set(t));
}

#[cfg(test)]
fn hook_table() -> Option<&'static Table> {
    TEST_TABLE.with(|c| c.get())
}

/// 번역 훅이 켜져 있는가(ko 이고 기계 모드가 아님). 번역표는 적재하지 않는다.
#[cfg(test)]
pub(crate) fn hooks_active() -> bool {
    TEST_TABLE.with(|c| c.get().is_some())
}

/// 번역 훅이 켜져 있는가(ko 이고 기계 모드가 아님). 번역표는 적재하지 않는다.
#[cfg(not(test))]
pub(crate) fn hooks_active() -> bool {
    is_ko() && !MACHINE_MODE.load(Ordering::Relaxed)
}

#[cfg(not(test))]
fn hook_table() -> Option<&'static Table> {
    // En 경로는 OnceLock 확인만 하고 기계 모드 플래그는 Ko 일 때만 읽는다.
    match lang() {
        Lang::En => None,
        Lang::Ko => hook_table_for(Lang::Ko, MACHINE_MODE.load(Ordering::Relaxed)),
    }
}

/// ko 일 때만 번역표를 돌려준다(En 이면 None). 기계 모드를 보지 않으므로 parse_cli/defer 단계(기계 모드 설정 전) 전용이다.
/// 그 밖의 출력은 반드시 t/t_pad/tr_line/tr_text/tr_error_report 훅을 쓴다.
pub(crate) fn table() -> Option<&'static Table> {
    table_for(lang())
}

/// `[[text]]` 완전 일치. 공백 없는 정적 라벨용. 없거나 En 이면 입력 그대로.
pub(crate) fn t(en: &'static str) -> &'static str {
    match hook_table() {
        Some(tb) => tb.text(en).unwrap_or(en),
        None => en,
    }
}

/// 앞뒤 공백이 있는 TUI 라벨용. 핵심부만 번역하고 원래 표시 폭에 맞춰 끝 공백을 다시 채운다.
pub(crate) fn t_pad(en_padded: &'static str) -> Cow<'static, str> {
    match hook_table() {
        Some(tb) => tb.t_pad(en_padded),
        None => Cow::Borrowed(en_padded),
    }
}

/// 한 줄 번역: 줄 전체 `[[text]]`, 앞뒤 공백을 뗀 `[[text]]`, `[[fmt]]` 순. 앞뒤 공백 보존.
pub(crate) fn tr_line(line: &str) -> Cow<'_, str> {
    match hook_table() {
        Some(tb) => tb.tr_line(line),
        None => Cow::Borrowed(line),
    }
}

/// 여러 줄 번역. 패닉 메시지("Thread … panicked at")는 그대로 둔다.
pub(crate) fn tr_text(text: &str) -> Cow<'_, str> {
    match hook_table() {
        Some(tb) => tb.tr_text(text),
        None => Cow::Borrowed(text),
    }
}

/// anyhow `{:?}` 보고서 번역("Stack backtrace:" 이하는 그대로).
pub(crate) fn tr_error_report(report: &str) -> Cow<'_, str> {
    match hook_table() {
        Some(tb) => tb.tr_error_report(report),
        None => Cow::Borrowed(report),
    }
}

/// 터미널 표시 폭. 동아시아 넓은 문자는 2칸, 나머지는 1칸.
pub(crate) fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| match c as u32 {
            // 자모 3130-318F 는 CJK 범위(2E80-9FFF)에 포함된다
            0x1100..=0x115F
            | 0x2E80..=0x9FFF
            | 0xAC00..=0xD7A3
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6 => 2,
            _ => 1,
        })
        .sum()
}

// ---------------------------------------------------------------------------------------------
// 번역표
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableError {
    /// 문제가 된 항목의 en (알 수 없으면 빈 문자열)
    pub en: String,
    pub msg: String,
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.en.is_empty() {
            write!(f, "{}", self.msg)
        } else {
            write!(f, "{} (en = {:?})", self.msg, self.en)
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTable {
    #[serde(default)]
    text: Vec<RawEntry>,
    #[serde(default)]
    fmt: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    en: String,
    ko: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Text,
    Fmt,
}

struct FmtEntry {
    pattern: Pattern,
    /// None 이면 ko = en(번역 불필요). 우선순위 경쟁에는 참여하고, 이기면 입력을 그대로 돌려준다.
    template: Option<Template>,
}

#[derive(Default)]
pub(crate) struct Table {
    text: HashMap<String, String>,
    fmt: Vec<FmtEntry>,
}

/// 한 줄 기본 문자열 규칙 위반을 소스 줄 단위로 찾는다. 위반이 속한 (종류, 순번) 항목을 돌려준다.
fn scan_one_line_rule(src: &str, errors: &mut Vec<TableError>) -> HashSet<(Kind, usize)> {
    let mut bad = HashSet::new();
    let mut cur: Option<(Kind, usize)> = None;
    let (mut n_text, mut n_fmt) = (0usize, 0usize);
    for (no, line) in src.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let compact: String = l.split_whitespace().collect();
        if compact.starts_with("[[text]]") {
            cur = Some((Kind::Text, n_text));
            n_text += 1;
            continue;
        }
        if compact.starts_with("[[fmt]]") {
            cur = Some((Kind::Fmt, n_fmt));
            n_fmt += 1;
            continue;
        }
        for key in ["en", "ko"] {
            let Some(rest) = l.strip_prefix(key) else {
                continue;
            };
            let Some(value) = rest.trim_start().strip_prefix('=') else {
                continue;
            };
            let v = value.trim_start();
            if !v.starts_with('"') || v.starts_with("\"\"\"") {
                if let Some(c) = cur {
                    bad.insert(c);
                }
                errors.push(TableError {
                    en: String::new(),
                    msg: format!("line {}: only one-line basic strings are allowed", no + 1),
                });
            }
        }
    }
    bad
}

fn has_numeric_ref(ko: &str) -> bool {
    let b = ko.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < b.len() && b[j] == b'}' {
                return true;
            }
        }
        i += 1;
    }
    false
}

impl Table {
    /// 오류가 있는 항목은 건너뛰고 오류 목록을 함께 돌려준다. TOML 자체가 깨졌으면 빈 표.
    pub(crate) fn parse(src: &str) -> (Table, Vec<TableError>) {
        let mut errors = Vec::new();
        let bad = scan_one_line_rule(src, &mut errors);
        let raw: RawTable = match toml::from_str(src) {
            Ok(r) => r,
            Err(e) => {
                errors.push(TableError {
                    en: String::new(),
                    msg: format!("TOML parse failure: {e}"),
                });
                return (Table::default(), errors);
            }
        };
        let mut table = Table::default();
        let mut seen: HashSet<String> = HashSet::new();
        for (i, e) in raw.text.iter().enumerate() {
            if !bad.contains(&(Kind::Text, i)) {
                table.add_text(e, &mut seen, &mut errors);
            }
        }
        for (i, e) in raw.fmt.iter().enumerate() {
            if !bad.contains(&(Kind::Fmt, i)) {
                table.add_fmt(e, &mut seen, &mut errors);
            }
        }
        (table, errors)
    }

    fn add_text(&mut self, e: &RawEntry, seen: &mut HashSet<String>, errors: &mut Vec<TableError>) {
        let mut fail = |msg: &str| {
            errors.push(TableError {
                en: e.en.clone(),
                msg: msg.to_string(),
            })
        };
        if e.en.is_empty() {
            return fail("empty en");
        }
        if !seen.insert(e.en.clone()) {
            return fail("duplicate en");
        }
        if e.ko.is_empty() {
            return; // 미번역
        }
        if has_numeric_ref(&e.ko) {
            return fail("[[text]] ko must not contain {n}");
        }
        self.text.insert(e.en.clone(), e.ko.clone());
    }

    fn add_fmt(&mut self, e: &RawEntry, seen: &mut HashSet<String>, errors: &mut Vec<TableError>) {
        let mut fail = |msg: &str| {
            errors.push(TableError {
                en: e.en.clone(),
                msg: msg.to_string(),
            })
        };
        let en = e.en.trim();
        if en.is_empty() {
            return fail("empty en");
        }
        if !seen.insert(en.to_string()) {
            return fail("duplicate en");
        }
        let pattern = match parse_rust_fmt(en) {
            Ok(p) => p,
            Err(err) => return fail(&format!("bad en format string: {err}")),
        };
        if pattern.groups() == 0 {
            return fail("[[fmt]] en has no placeholder (use [[text]])");
        }
        if pattern.literal_alpha() < MIN_LITERAL_ALPHA {
            return fail("[[fmt]] en has too few literal letters (need >= 4)");
        }
        if pattern.is_open() && pattern.literal_len() < MIN_OPEN_LITERAL_LEN {
            return fail("[[fmt]] en has an open end and too few literal characters (need >= 8)");
        }
        if e.ko.is_empty() {
            return; // 미번역
        }
        if e.ko == e.en {
            // ko = en(번역 불필요): 더 덜 구체적인 번역 항목이 이 줄을 가로채지 못하게 경쟁에는 참여시킨다.
            self.fmt.push(FmtEntry {
                pattern,
                template: None,
            });
            return;
        }
        let template = match parse_ko_template(&e.ko, pattern.groups()) {
            Ok(t) => t,
            Err(err) => return fail(&format!("bad ko template: {err}")),
        };
        if pattern.ends_open() && template.last_ref() != Some(pattern.groups()) {
            return fail(
                "en ends with a value group, so ko must end with the last group reference",
            );
        }
        if pattern.starts_open() && template.first_ref() != Some(1) {
            return fail("en starts with a value group, so ko must start with {1}");
        }
        self.fmt.push(FmtEntry {
            pattern,
            template: Some(template),
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty() && self.fmt.is_empty()
    }

    pub(crate) fn text(&self, en: &str) -> Option<&str> {
        self.text.get(en).map(String::as_str)
    }

    /// 앞뒤 공백을 뗀 핵심부를 `[[text]]` 로 조회하고 앞 공백 + 번역 + 원래 폭에 맞춘 끝 공백을 붙인다.
    pub(crate) fn t_pad<'a>(&self, en_padded: &'a str) -> Cow<'a, str> {
        let core = en_padded.trim();
        if core.is_empty() {
            return Cow::Borrowed(en_padded);
        }
        let Some(ko) = self.text.get(core) else {
            return Cow::Borrowed(en_padded);
        };
        if ko == core {
            return Cow::Borrowed(en_padded);
        }
        let lead = &en_padded[..en_padded.len() - en_padded.trim_start().len()];
        let trail = &en_padded[en_padded.trim_end().len()..];
        let mut out = String::with_capacity(en_padded.len() + ko.len());
        out.push_str(lead);
        out.push_str(ko);
        if !trail.is_empty() {
            let target = display_width(en_padded);
            let pad = target.saturating_sub(display_width(&out)).max(1);
            out.extend(std::iter::repeat_n(' ', pad));
        }
        Cow::Owned(out)
    }

    pub(crate) fn tr_line<'a>(&self, line: &'a str) -> Cow<'a, str> {
        if self.is_empty() {
            return Cow::Borrowed(line);
        }
        if let Some(ko) = self.text.get(line) {
            if ko == line {
                return Cow::Borrowed(line);
            }
            return Cow::Owned(ko.clone());
        }
        let core = line.trim();
        if core.is_empty() {
            return Cow::Borrowed(line);
        }
        let lead = &line[..line.len() - line.trim_start().len()];
        let trail = &line[line.trim_end().len()..];
        if let Some(ko) = self.text.get(core) {
            if ko == core {
                return Cow::Borrowed(line);
            }
            return Cow::Owned(format!("{lead}{ko}{trail}"));
        }
        let mut best: Option<(&FmtEntry, Vec<&str>)> = None;
        for e in &self.fmt {
            let Some(caps) = e.pattern.captures(core) else {
                continue;
            };
            let better = match &best {
                None => true,
                Some((b, _)) => {
                    let (l, g) = (e.pattern.literal_len(), e.pattern.groups());
                    let (bl, bg) = (b.pattern.literal_len(), b.pattern.groups());
                    l > bl || (l == bl && g < bg)
                }
            };
            if better {
                best = Some((e, caps));
            }
        }
        match best {
            Some((
                FmtEntry {
                    template: Some(tpl),
                    ..
                },
                caps,
            )) => Cow::Owned(format!("{lead}{}{trail}", tpl.render(&caps))),
            _ => Cow::Borrowed(line),
        }
    }

    pub(crate) fn tr_text<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.is_empty() {
            return Cow::Borrowed(text);
        }
        let first = text.split('\n').next().unwrap_or("");
        if first.starts_with("Thread ") && first.contains(" panicked at ") {
            return Cow::Borrowed(text);
        }
        // 바뀐 줄이 처음 나올 때까지는 할당하지 않는다(앞부분은 원문 그대로 복사).
        let mut out: Option<String> = None;
        let mut pos = 0usize;
        for l in text.split('\n') {
            let start = pos;
            pos += l.len() + 1;
            let tr = self.tr_line(l);
            match (&mut out, tr) {
                (None, Cow::Borrowed(_)) => {}
                (None, Cow::Owned(o)) => {
                    let mut s = String::with_capacity(text.len());
                    s.push_str(&text[..start]);
                    s.push_str(&o);
                    out = Some(s);
                }
                (Some(s), tr) => {
                    s.push('\n');
                    s.push_str(&tr);
                }
            }
        }
        match out {
            Some(s) => Cow::Owned(s),
            None => Cow::Borrowed(text),
        }
    }

    pub(crate) fn tr_error_report<'a>(&self, r: &'a str) -> Cow<'a, str> {
        if self.is_empty() {
            return Cow::Borrowed(r);
        }
        let mut out = String::with_capacity(r.len());
        let mut changed = false;
        let mut stop = false;
        for (i, l) in r.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if stop {
                out.push_str(l);
                continue;
            }
            if l.trim() == "Stack backtrace:" {
                stop = true;
                out.push_str(l);
                continue;
            }
            let (prefix, rest) = split_numbered(l).unwrap_or(("", l));
            match self.tr_line(rest) {
                Cow::Borrowed(_) => out.push_str(l),
                Cow::Owned(o) => {
                    changed = true;
                    out.push_str(prefix);
                    out.push_str(&o);
                }
            }
        }
        if changed {
            Cow::Owned(out)
        } else {
            Cow::Borrowed(r)
        }
    }
}

/// `^(\s+)(\d+): (.*)$` 의 접두부(`    0: `)와 나머지로 나눈다.
fn split_numbered(l: &str) -> Option<(&str, &str)> {
    let t = l.trim_start();
    let lead = l.len() - t.len();
    if lead == 0 {
        return None;
    }
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || !t[digits..].starts_with(": ") {
        return None;
    }
    let cut = lead + digits + 2;
    Some((&l[..cut], &l[cut..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tbl(src: &str) -> Table {
        let (t, errs) = Table::parse(src);
        assert!(errs.is_empty(), "unexpected table errors: {errs:?}");
        t
    }

    const SAMPLE: &str = r#"
[[text]]
en = "Build the project"
ko = "프로젝트를 빌드합니다"

[[text]]
en = "Caused by:"
ko = "원인:"

[[text]]
en = "App:"
ko = "앱:"

[[fmt]]
en = "Serving app at {}"
ko = "앱을 제공합니다: {1}"

[[fmt]]
en = "hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms"
ko = "{1}ms 만에 핫 패치했습니다"

[[fmt]]
en = "Failed to read {} from {}"
ko = "{1}을(를) 읽지 못했습니다: {2}"

[[fmt]]
en = "Build failed: {}"
ko = "빌드에 실패했습니다: {1}"
"#;

    #[test]
    fn parse_lang_cases() {
        assert_eq!(parse_lang(None), Lang::Ko);
        assert_eq!(parse_lang(Some("")), Lang::Ko);
        assert_eq!(parse_lang(Some("en")), Lang::En);
        assert_eq!(parse_lang(Some("EN")), Lang::En);
        assert_eq!(parse_lang(Some("en_US.UTF-8")), Lang::En);
        assert_eq!(parse_lang(Some("en-GB")), Lang::En);
        assert_eq!(parse_lang(Some("ko")), Lang::Ko);
        assert_eq!(parse_lang(Some("ko_KR.UTF-8")), Lang::Ko);
        assert_eq!(parse_lang(Some("fr")), Lang::Ko);
    }

    #[test]
    fn display_width_cases() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("앱:"), 3);
        assert_eq!(display_width("가나다"), 6);
        assert_eq!(display_width("ㄱ"), 2);
        assert_eq!(display_width("漢字"), 4);
        assert_eq!(display_width("ＡＢ"), 4);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn global_hooks_pass_through_in_en() {
        // cfg(test) 에서 전역 언어는 En
        assert_eq!(lang(), Lang::En);
        assert!(!is_ko());
        assert_eq!(t("Build the project"), "Build the project");
        assert!(matches!(t_pad("App:    "), Cow::Borrowed("App:    ")));
        assert!(matches!(tr_line("Serving app at x"), Cow::Borrowed(_)));
        assert!(matches!(tr_text("a\nb"), Cow::Borrowed(_)));
        assert!(matches!(tr_error_report("x"), Cow::Borrowed(_)));
    }

    // 주의: 이 테스트 외에는 어떤 테스트도 table_for(Lang::Ko)/KO_TABLE 을 부르면 안 된다(LOADS 절대값 검사).
    // 다른 테스트는 Box::leak(Table::parse(..).0) 를 쓴다.
    #[test]
    fn loads_only_when_ko_actually_parses() {
        // 전역 훅은 테스트에서 En 이므로 LOADS 를 올릴 수 있는 것은 이 테스트의 table_for(Ko) 뿐이다.
        assert_eq!(LOADS.load(Ordering::SeqCst), 0);
        assert!(table_for(Lang::En).is_none());
        assert!(table().is_none());
        let _ = t("x");
        let _ = tr_line("x");
        assert_eq!(LOADS.load(Ordering::SeqCst), 0);
        let a = table_for(Lang::Ko).unwrap() as *const Table;
        let b = table_for(Lang::Ko).unwrap() as *const Table;
        assert_eq!(a, b);
        assert_eq!(LOADS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn machine_mode_disables_hooks() {
        assert!(hook_table_for(Lang::Ko, true).is_none());
        assert!(hook_table_for(Lang::En, false).is_none());
    }

    #[test]
    fn text_lookup_and_fmt_basic() {
        let t = tbl(SAMPLE);
        assert_eq!(t.text("Build the project"), Some("프로젝트를 빌드합니다"));
        assert_eq!(t.tr_line("Build the project"), "프로젝트를 빌드합니다");
        assert_eq!(
            t.tr_line("Serving app at http://x"),
            "앱을 제공합니다: http://x"
        );
        assert_eq!(
            t.tr_line("Failed to read a.txt from /tmp"),
            "a.txt을(를) 읽지 못했습니다: /tmp"
        );
    }

    #[test]
    fn style_group_value_is_preserved() {
        let t = tbl(SAMPLE);
        assert_eq!(
            t.tr_line("hotpatched in \x1b[1m12\x1b[0mms"),
            "\x1b[1m12\x1b[0mms 만에 핫 패치했습니다"
        );
    }

    #[test]
    fn surrounding_whitespace_is_preserved() {
        let t = tbl(SAMPLE);
        assert_eq!(t.tr_line("  Serving app at x  "), "  앱을 제공합니다: x  ");
        assert_eq!(
            t.tr_line("\tBuild the project\r"),
            "\t프로젝트를 빌드합니다\r"
        );
    }

    #[test]
    fn mismatch_is_borrowed() {
        let t = tbl(SAMPLE);
        assert!(matches!(t.tr_line("Something unrelated"), Cow::Borrowed(_)));
        assert!(matches!(t.tr_line(""), Cow::Borrowed(_)));
        assert!(matches!(t.tr_line("   "), Cow::Borrowed(_)));
        assert!(matches!(t.tr_text("no match\nat all"), Cow::Borrowed(_)));
        assert!(matches!(
            Table::default().tr_line("Serving app at x"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn translating_korean_again_is_idempotent() {
        let t = tbl(SAMPLE);
        let once = t.tr_line("Serving app at x").into_owned();
        assert_eq!(t.tr_line(&once), once.as_str());
        let text = t
            .tr_text("Build the project\nServing app at y\n")
            .into_owned();
        assert_eq!(t.tr_text(&text), text.as_str());
    }

    #[test]
    fn specificity_longer_literal_wins() {
        let t = tbl(r#"
[[fmt]]
en = "Failed to {}"
ko = "짧은 {1}"

[[fmt]]
en = "Failed to read {}"
ko = "긴 {1}"
"#);
        assert_eq!(t.tr_line("Failed to read x"), "긴 x");
        assert_eq!(t.tr_line("Failed to write x"), "짧은 write x");
    }

    #[test]
    fn specificity_ties_prefer_fewer_groups_then_file_order() {
        // 리터럴 길이 12 로 같고 묶음 수가 다른 두 패턴: 묶음이 적은 쪽이 이긴다
        let t = tbl(r#"
[[fmt]]
en = "Alpha {} beta {}"
ko = "둘 {1} {2}"

[[fmt]]
en = "Alph{} beta 22"
ko = "하나 {1}"
"#);
        assert_eq!(t.tr_line("Alpha 1 beta 22"), "하나 a 1");
        // 리터럴 길이도 묶음 수도 같으면 파일에서 앞선 항목
        let first = "[[fmt]]\nen = \"Alpha {} beta {}\"\nko = \"앞 {1} {2}\"\n\n[[fmt]]\nen = \"Alpha{} beta {}2\"\nko = \"뒤 {1} {2}\"\n";
        assert_eq!(tbl(first).tr_line("Alpha 1 beta 22"), "앞 1 22");
        let swapped = "[[fmt]]\nen = \"Alpha{} beta {}2\"\nko = \"뒤 {1} {2}\"\n\n[[fmt]]\nen = \"Alpha {} beta {}\"\nko = \"앞 {1} {2}\"\n";
        assert_eq!(tbl(swapped).tr_line("Alpha 1 beta 22"), "뒤  1 2");
        // 같은 en 은 중복 오류
        let (_, errs) = Table::parse(
            "[[fmt]]\nen = \"Same text {} one {}\"\nko = \"첫째 {1} {2}\"\n\n[[fmt]]\nen = \"Same text {} one {}\"\nko = \"둘째 {1} {2}\"\n",
        );
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn group_order_can_be_swapped_and_repeated() {
        let t = tbl(r#"
[[fmt]]
en = "Copied {} into {} now"
ko = "{2}에 {1}을(를) 복사했습니다 ({1})"
"#);
        assert_eq!(
            t.tr_line("Copied a into b now"),
            "b에 a을(를) 복사했습니다 (a)"
        );
    }

    #[test]
    fn template_with_escaped_brace() {
        let t = tbl(r#"
[[fmt]]
en = "Found config {} here"
ko = "{{{1}}} 설정을 찾았습니다"
"#);
        assert_eq!(t.tr_line("Found config x here"), "{x} 설정을 찾았습니다");
    }

    #[test]
    fn tr_text_multiline_keeps_newlines() {
        let t = tbl(SAMPLE);
        let out = t.tr_text("Build the project\n\n  Serving app at x\nplain line\n");
        assert_eq!(
            out,
            "프로젝트를 빌드합니다\n\n  앱을 제공합니다: x\nplain line\n"
        );
        assert_eq!(t.tr_text("Build the project"), "프로젝트를 빌드합니다");
    }

    #[test]
    fn fmt_identity_is_skipped_without_error() {
        let (t, errs) = Table::parse(
            "[[fmt]]\nen = \"Serving app at {}\"\nko = \"Serving app at {}\"\n\n[[fmt]]\nen = \"hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms\"\nko = \"hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms\"\n",
        );
        assert!(errs.is_empty(), "{errs:?}");
        assert!(matches!(t.tr_line("Serving app at x"), Cow::Borrowed(_)));
        assert!(matches!(
            t.tr_line("hotpatched in 3ms"),
            Cow::Borrowed("hotpatched in 3ms")
        ));
    }

    #[test]
    fn fmt_identity_wins_over_less_specific_translation() {
        let t = tbl(r#"
[[fmt]]
en = "Failed to {}"
ko = "짧은 {1}"

[[fmt]]
en = "Failed to read {}"
ko = "Failed to read {}"
"#);
        assert!(matches!(
            t.tr_line("  Failed to read x"),
            Cow::Borrowed("  Failed to read x")
        ));
        assert_eq!(t.tr_line("Failed to write x"), "짧은 write x");
    }

    #[test]
    fn text_identity_is_borrowed() {
        let t = tbl("[[text]]\nen = \"Same label\"\nko = \"Same label\"\n");
        assert!(matches!(t.tr_line("Same label"), Cow::Borrowed(_)));
        assert!(matches!(t.tr_line("  Same label "), Cow::Borrowed(_)));
        assert!(matches!(t.t_pad("Same label  "), Cow::Borrowed(_)));
    }

    #[test]
    fn spaced_headers_keep_entry_numbering() {
        let (t, errs) = Table::parse(
            "[[ text ]]\nen = \"Bad one\"\nko = \"\"\"x\"\"\"\n\n[[text]]\nen = \"Good one\"\nko = \"좋음\"\n",
        );
        assert!(!errs.is_empty());
        assert_eq!(t.text("Good one"), Some("좋음"));
    }

    #[test]
    fn tr_text_lazy_copy_matches_eager() {
        let t = tbl(SAMPLE);
        assert!(matches!(t.tr_text("a\nb\n"), Cow::Borrowed(_)));
        assert_eq!(
            t.tr_text("keep\nBuild the project\nkeep2\n"),
            "keep\n프로젝트를 빌드합니다\nkeep2\n"
        );
        assert_eq!(
            t.tr_text("Build the project\nx"),
            "프로젝트를 빌드합니다\nx"
        );
    }

    #[test]
    fn tr_text_protects_panic_messages() {
        let t = tbl(SAMPLE);
        let p = "Thread main panicked at src/x.rs:1:1:\nBuild failed: boom";
        assert!(matches!(t.tr_text(p), Cow::Borrowed(_)));
        // 첫 줄이 패닉이 아니면 번역한다
        assert_eq!(
            t.tr_text("note\nBuild failed: boom"),
            "note\n빌드에 실패했습니다: boom"
        );
    }

    #[test]
    fn error_report_caused_by_single_and_numbered() {
        let t = tbl(SAMPLE);
        let single = "Build failed: top\n\nCaused by:\n    Serving app at z";
        assert_eq!(
            t.tr_error_report(single),
            "빌드에 실패했습니다: top\n\n원인:\n    앱을 제공합니다: z"
        );
        let numbered = "Build failed: top\n\nCaused by:\n    0: Serving app at a\n    1: Failed to read q from w";
        assert_eq!(
            t.tr_error_report(numbered),
            "빌드에 실패했습니다: top\n\n원인:\n    0: 앱을 제공합니다: a\n    1: q을(를) 읽지 못했습니다: w"
        );
    }

    #[test]
    fn error_report_leaves_backtrace_alone() {
        let t = tbl(SAMPLE);
        let r = "Build failed: x\n\nStack backtrace:\n   0: Serving app at y\n   1: Caused by:";
        assert_eq!(
            t.tr_error_report(r),
            "빌드에 실패했습니다: x\n\nStack backtrace:\n   0: Serving app at y\n   1: Caused by:"
        );
        assert!(matches!(
            t.tr_error_report("nothing here"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn duplicate_keys_are_errors_across_kinds() {
        let (t, errs) = Table::parse(
            r#"
[[text]]
en = "Hello there {}"
ko = "하나"

[[fmt]]
en = "Hello there {}"
ko = "둘 {1}"

[[text]]
en = "Same"
ko = "가"

[[text]]
en = "Same"
ko = "나"
"#,
        );
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert_eq!(t.text("Same"), Some("가"));
        assert!(t.fmt.is_empty());
    }

    #[test]
    fn empty_ko_is_skipped_without_error() {
        let (t, errs) = Table::parse(
            r#"
[[text]]
en = "Untranslated text"
ko = ""

[[fmt]]
en = "Untranslated {} value"
ko = ""
"#,
        );
        assert!(errs.is_empty(), "{errs:?}");
        assert!(t.is_empty());
        assert_eq!(t.tr_line("Untranslated text"), "Untranslated text");
    }

    #[test]
    fn bad_fmt_entries_are_reported_and_skipped() {
        let (t, errs) = Table::parse(
            r#"
[[fmt]]
en = "{}: {}"
ko = "{1}: {2}"

[[fmt]]
en = "Reading {} now"
ko = "{3}"

[[fmt]]
en = "Reading {} twice {}"
ko = "{1}"

[[fmt]]
en = "No placeholder here"
ko = "없음"

[[fmt]]
en = "Broken {"
ko = "깨짐"

[[fmt]]
en = "Good {} entry"
ko = "좋은 {1} 항목"
"#,
        );
        assert_eq!(errs.len(), 5, "{errs:?}");
        assert_eq!(t.fmt.len(), 1);
    }

    #[test]
    fn open_ended_patterns_require_matching_ko_ends() {
        let (t, errs) = Table::parse(
            r#"
[[fmt]]
en = "Failed to read {}"
ko = "{1}을(를) 읽지 못했습니다"

[[fmt]]
en = "{} was not found anywhere"
ko = "찾지 못했습니다: {1}"

[[fmt]]
en = "Failed to open {} for {}"
ko = "{2} 때문에 {1} 열기 실패"

[[fmt]]
en = "Failed to write {}"
ko = "쓰기 실패: {1}"

[[fmt]]
en = "{} was found nearby"
ko = "{1} 근처에서 찾았습니다"
"#,
        );
        // 1: 끝이 열린데 ko 가 참조로 끝나지 않음, 2: 시작이 열린데 {1} 로 시작 안 함, 3: 끝 참조가 {2} 아님
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert_eq!(t.fmt.len(), 2);
    }

    #[test]
    fn one_line_string_rule_is_enforced() {
        let (t, errs) = Table::parse(
            "[[text]]\nen = \"Multi line text\"\nko = \"\"\"\n여러 줄\n\"\"\"\n\n[[text]]\nen = 'Literal string'\nko = \"리터럴\"\n\n[[text]]\nen = \"Fine text\"\nko = \"괜찮음\"\n",
        );
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert_eq!(t.text("Fine text"), Some("괜찮음"));
        assert_eq!(t.text("Multi line text"), None);
        assert_eq!(t.text("Literal string"), None);
    }

    #[test]
    fn unknown_fields_and_broken_toml_give_empty_table() {
        let (t, errs) = Table::parse("[[text]]\nen = \"x\"\nko = \"y\"\nextra = \"z\"\n");
        assert!(t.is_empty());
        assert_eq!(errs.len(), 1);
        let (t, errs) = Table::parse("[[text\nen = ");
        assert!(t.is_empty());
        assert!(!errs.is_empty());
    }

    #[test]
    fn text_ko_must_not_contain_numeric_ref() {
        let (_, errs) = Table::parse("[[text]]\nen = \"Plain label\"\nko = \"값 {1}\"\n");
        assert_eq!(errs.len(), 1);
        let (t, errs) = Table::parse("[[text]]\nen = \"Plain label\"\nko = \"중괄호 {x}\"\n");
        assert!(errs.is_empty());
        assert_eq!(t.text("Plain label"), Some("중괄호 {x}"));
    }

    #[test]
    fn t_pad_keeps_display_width() {
        let t = tbl(SAMPLE);
        // "App:    " 폭 8 -> "앱:" 폭 3 + 5칸
        let out = t.t_pad("App:    ");
        assert_eq!(out, "앱:     ");
        assert_eq!(display_width(&out), display_width("App:    "));
        // 선행 공백 보존
        assert_eq!(t.t_pad("  App: "), "  앱:  ");
        // 번역이 더 넓어도 최소 1칸
        let wide = tbl("[[text]]\nen = \"OK\"\nko = \"확인했습니다\"\n");
        assert_eq!(wide.t_pad("OK "), "확인했습니다 ");
        // 끝 공백이 원래 없으면 붙이지 않음
        assert_eq!(wide.t_pad(" OK"), " 확인했습니다");
        // 번역 없음은 그대로
        assert!(matches!(t.t_pad("Nope   "), Cow::Borrowed("Nope   ")));
    }

    #[test]
    fn open_end_needs_eight_literal_chars() {
        // 열린 끝 + 리터럴 5자: 건너뜀
        let (t, errs) = Table::parse("[[fmt]]\nen = \"from {libssl}\"\nko = \"출처 {1}\"\n");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(t.fmt.is_empty());
        // 전체 글자 수(공백·문장부호 포함) 기준: 7자는 거부, 8자 이상은 허용
        for (en, ok) in [
            ("changed{extra}", false),
            ("XCode: {}", false),
            ("xcrun: {}", false),
            ("Signing {}", true),
            ("Warning: {}", true),
            ("cc path: {}", true),
            ("- Yours:  {}", true),
            ("- Latest: {}", true),
        ] {
            let src = format!("[[fmt]]\nen = \"{en}\"\nko = \"값: {{1}}\"\n");
            let (_, errs) = Table::parse(&src);
            assert_eq!(errs.is_empty(), ok, "{en}: {errs:?}");
        }
        // 열린 끝 + 8자 이상: 허용
        let (_, errs) =
            Table::parse("[[fmt]]\nen = \"Loaded from {src}\"\nko = \"{1}에서 불러왔습니다\"\n");
        assert_eq!(errs.len(), 1, "ko must start with {{1}}: {errs:?}");
        let (t, errs) = Table::parse("[[fmt]]\nen = \"Loaded from {src}\"\nko = \"출처: {1}\"\n");
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(t.fmt.len(), 1);
        // 닫힌 패턴은 알파벳 4자면 충분
        let (_, errs) = Table::parse("[[fmt]]\nen = \"[{}] done [{}]\"\nko = \"[{1}] 끝 [{2}]\"\n");
        assert!(errs.is_empty(), "{errs:?}");
        // 스타일 전용 묶음은 열린 끝이 아니다
        let (_, errs) = Table::parse(
            "[[fmt]]\nen = \"{LINK_STYLE}Setup{LINK_STYLE:#}\"\nko = \"{1}설정{2}\"\n",
        );
        assert!(errs.is_empty(), "{errs:?}");
        // 앞이 스타일, 끝이 일반 값이면 열린 끝: 알파벳 부족 시 건너뜀
        let (_, errs) = Table::parse(
            "[[fmt]]\nen = \"{ERROR_STYLE}Fail{ERROR_STYLE:#}: {}\"\nko = \"{1}실패{2}: {3}\"\n",
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
    }

    #[test]
    fn toggle_lines_use_synthesized_text_keys_before_fmt() {
        let t = tbl(
            "[[fmt]]\nen = \"Verbose logging is now {}\"\nko = \"자세한 로그 상태: {1}\"\n\
             [[text]]\nen = \"Verbose logging is now on\"\nko = \"자세한 로그: 켜짐\"\n",
        );
        assert_eq!(t.tr_line("Verbose logging is now on"), "자세한 로그: 켜짐");
        assert_eq!(
            t.tr_line("Verbose logging is now on "),
            "자세한 로그: 켜짐 "
        );
        // 번역표에 없는 값은 fmt 로
        assert_eq!(
            t.tr_line("Verbose logging is now maybe"),
            "자세한 로그 상태: maybe"
        );
        // 실제 번역표 (번역표가 빈 머리글뿐이면 건너뛴다)
        let (ko, errs) = Table::parse(KO_SRC);
        assert!(errs.is_empty(), "{errs:?}");
        if ko.is_empty() {
            return;
        }
        for (en, want) in [
            ("Verbose logging is now on", "자세한 로그: 켜짐"),
            ("Verbose logging is now off", "자세한 로그: 꺼짐"),
            ("Tracing is now on", "추적 로그: 켜짐"),
            ("Tracing is now off ", "추적 로그: 꺼짐 "),
        ] {
            assert_eq!(ko.tr_line(en), want, "{en}");
        }
    }

    #[test]
    fn ko_table_is_valid() {
        let (_, errs) = Table::parse(KO_SRC);
        assert!(errs.is_empty(), "ko.toml errors: {errs:#?}");
    }
}
