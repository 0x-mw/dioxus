//! Rust 서식 문자열 파서, 패턴 매칭, 한국어 템플릿. 순수 함수만 둔다(정규식 없음).

use std::fmt;

/// `[[fmt]]` 항목이 가져야 하는 리터럴 알파벳의 최소 개수.
pub(crate) const MIN_LITERAL_ALPHA: usize = 4;

/// 열린 끝(스타일 전용 묶음을 뺀 첫·끝 조각이 일반 값 묶음) 패턴이 가져야 하는 리터럴의 최소 전체 글자 수(공백·문장부호 포함).
pub(crate) const MIN_OPEN_LITERAL_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Seg {
    Lit(String),
    /// 인접한 자리표시자는 값 묶음 1개로 병합된다. `style_only` 는 묶음의 모든 인자 이름이
    /// `_STYLE` 로 끝나는 경우(스타일 상수)다.
    Group {
        style_only: bool,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Pattern {
    segs: Vec<Seg>,
    literal_len: usize,
    groups: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FmtError(pub String);

impl fmt::Display for FmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FmtError {}

fn err<T>(msg: impl Into<String>) -> Result<T, FmtError> {
    Err(FmtError(msg.into()))
}

/// `{}` 안의 인자 부분(`:` 앞)이 비어 있거나 숫자 또는 식별자(`r#type` 포함)인지 확인한다.
fn check_arg(inner: &str) -> Result<bool, FmtError> {
    let arg = inner.split(':').next().unwrap_or("");
    if arg
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '#')
    {
        Ok(arg.ends_with("_STYLE"))
    } else {
        err(format!("invalid placeholder argument in `{{{inner}}}`"))
    }
}

pub(crate) fn parse_rust_fmt(fmt: &str) -> Result<Pattern, FmtError> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut lit = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    lit.push('{');
                    continue;
                }
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') => return err("nested `{` inside placeholder"),
                        Some(ch) => inner.push(ch),
                        None => return err("unclosed `{`"),
                    }
                }
                let is_style = check_arg(&inner)?;
                if !lit.is_empty() {
                    segs.push(Seg::Lit(std::mem::take(&mut lit)));
                }
                match segs.last_mut() {
                    Some(Seg::Group { style_only }) => *style_only &= is_style,
                    _ => segs.push(Seg::Group {
                        style_only: is_style,
                    }),
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    lit.push('}');
                } else {
                    return err("unmatched `}`");
                }
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        segs.push(Seg::Lit(lit));
    }
    let literal_len = segs
        .iter()
        .map(|s| match s {
            Seg::Lit(l) => l.chars().count(),
            Seg::Group { .. } => 0,
        })
        .sum();
    let groups = segs
        .iter()
        .filter(|s| matches!(s, Seg::Group { .. }))
        .count();
    Ok(Pattern {
        segs,
        literal_len,
        groups,
    })
}

impl Pattern {
    pub(crate) fn literal_alpha(&self) -> usize {
        self.segs
            .iter()
            .map(|s| match s {
                Seg::Lit(l) => l.chars().filter(|c| c.is_alphabetic()).count(),
                Seg::Group { .. } => 0,
            })
            .sum()
    }

    pub(crate) fn literal_len(&self) -> usize {
        self.literal_len
    }

    pub(crate) fn groups(&self) -> usize {
        self.groups
    }

    /// 패턴이 값 묶음으로 시작하는가.
    pub(crate) fn starts_open(&self) -> bool {
        matches!(self.segs.first(), Some(Seg::Group { .. }))
    }

    /// 패턴이 값 묶음으로 끝나는가.
    pub(crate) fn ends_open(&self) -> bool {
        matches!(self.segs.last(), Some(Seg::Group { .. }))
    }

    /// 스타일 전용 묶음을 뺀 뒤 첫 조각이나 마지막 조각이 일반 값 묶음인가(열린 끝).
    pub(crate) fn is_open(&self) -> bool {
        let mut it = self
            .segs
            .iter()
            .filter(|s| !matches!(s, Seg::Group { style_only: true }));
        let first = it.next();
        let last = it.next_back().or(first);
        matches!(first, Some(Seg::Group { .. })) || matches!(last, Some(Seg::Group { .. }))
    }

    /// 앞뒤 공백을 뗀 줄 핵심부 `s` 가 패턴과 맞으면 묶음별 값을 돌려준다.
    /// 앞 고정, 뒤 고정, 중간 리터럴은 왼쪽부터 찾되, 뒤 조각이나 스타일 전용 묶음 검사가 실패하면
    /// 다음 위치로 되돌아간다(`^lit(.*?)lit..$` 와 같은 결과 = 가장 왼쪽 해). 시도 횟수 상한을 넘으면 None.
    /// 스타일 전용 묶음은 빈 문자열이거나 ANSI SGR 시퀀스로만 된 값만 잡는다.
    pub(crate) fn captures<'a>(&self, s: &'a str) -> Option<Vec<&'a str>> {
        let segs = &self.segs;
        match segs.len() {
            0 => return s.is_empty().then(Vec::new),
            1 => {
                return match &segs[0] {
                    Seg::Lit(l) => (s == l).then(Vec::new),
                    Seg::Group { style_only } => (!*style_only || is_sgr_only(s)).then(|| vec![s]),
                };
            }
            _ => {}
        }
        let mut lo = 0;
        let mut hi = s.len();
        let mut start = 0;
        let mut end = segs.len();
        if let Seg::Lit(l) = &segs[0] {
            if !s.starts_with(l.as_str()) {
                return None;
            }
            lo = l.len();
            start = 1;
        }
        if let Some(Seg::Lit(l)) = segs.last() {
            if !s.ends_with(l.as_str()) || s.len() - l.len() < lo {
                return None;
            }
            hi = s.len() - l.len();
            end -= 1;
        }
        let mut caps = Vec::with_capacity(self.groups);
        let mut budget = MAX_MATCH_STEPS;
        backtrack(s, &segs[start..end], lo, hi, None, &mut caps, &mut budget).then_some(caps)
    }
}

/// `captures` 의 중간 리터럴 탐색 시도 횟수 상한(최악 경우 폭발 방지).
const MAX_MATCH_STEPS: usize = 10_000;

/// `segs` 를 `s[pos..hi]` 에 맞춘다. `open` 은 아직 닫히지 않은 묶음의 (시작 위치, 스타일 전용 여부).
fn backtrack<'a>(
    s: &'a str,
    segs: &[Seg],
    pos: usize,
    hi: usize,
    open: Option<(usize, bool)>,
    caps: &mut Vec<&'a str>,
    budget: &mut usize,
) -> bool {
    match segs.split_first() {
        None => {
            if let Some((g, style_only)) = open {
                let cap = &s[g..hi];
                if style_only && !is_sgr_only(cap) {
                    return false;
                }
                caps.push(cap);
            }
            true
        }
        Some((Seg::Group { style_only }, rest)) => {
            backtrack(s, rest, pos, hi, Some((pos, *style_only)), caps, budget)
        }
        Some((Seg::Lit(l), rest)) => {
            let mut from = pos;
            while *budget > 0 {
                let Some(i) = s[from..hi].find(l.as_str()) else {
                    return false;
                };
                *budget -= 1;
                let at = from + i;
                let mut pushed = false;
                let mut usable = true;
                if let Some((g, style_only)) = open {
                    let cap = &s[g..at];
                    if style_only && !is_sgr_only(cap) {
                        usable = false;
                    } else {
                        caps.push(cap);
                        pushed = true;
                    }
                }
                if usable {
                    if backtrack(s, rest, at + l.len(), hi, None, caps, budget) {
                        return true;
                    }
                    if pushed {
                        caps.pop();
                    }
                }
                from = at + s[at..].chars().next().map_or(1, char::len_utf8);
            }
            false
        }
    }
}

/// 빈 문자열이거나 `\x1b[` … `m` 시퀀스로만 이루어졌는가.
fn is_sgr_only(s: &str) -> bool {
    let mut rest = s;
    while !rest.is_empty() {
        let Some(body) = rest.strip_prefix("\x1b[") else {
            return false;
        };
        let Some(end) = body.find(|c: char| !(c.is_ascii_digit() || c == ';' || c == ':')) else {
            return false;
        };
        if !body[end..].starts_with('m') {
            return false;
        }
        rest = &body[end + 1..];
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Lit(String),
    Ref(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct Template {
    parts: Vec<Part>,
}

/// 한국어 템플릿(`{1}`..`{groups}`, `{{`, `}}`)을 파싱한다. 범위 밖 번호, 그 밖의 `{`·`}`,
/// 사용하지 않은 묶음은 오류.
pub(crate) fn parse_ko_template(ko: &str, groups: usize) -> Result<Template, FmtError> {
    let mut parts: Vec<Part> = Vec::new();
    let mut lit = String::new();
    let mut used = vec![false; groups];
    let mut chars = ko.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    lit.push('{');
                    continue;
                }
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(d) if d.is_ascii_digit() => digits.push(d),
                        _ => return err("invalid `{` in ko template"),
                    }
                }
                let n: usize = match digits.parse() {
                    Ok(n) => n,
                    Err(_) => return err("invalid group reference in ko template"),
                };
                if n == 0 || n > groups {
                    return err(format!(
                        "group reference {{{n}}} out of range (1..={groups})"
                    ));
                }
                used[n - 1] = true;
                if !lit.is_empty() {
                    parts.push(Part::Lit(std::mem::take(&mut lit)));
                }
                parts.push(Part::Ref(n));
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    lit.push('}');
                } else {
                    return err("unmatched `}` in ko template");
                }
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        parts.push(Part::Lit(lit));
    }
    if let Some(i) = used.iter().position(|u| !u) {
        return err(format!("group {{{}}} is never used", i + 1));
    }
    Ok(Template { parts })
}

impl Template {
    pub(crate) fn render(&self, caps: &[&str]) -> String {
        let mut out = String::new();
        for p in &self.parts {
            match p {
                Part::Lit(l) => out.push_str(l),
                Part::Ref(n) => out.push_str(caps.get(n - 1).copied().unwrap_or("")),
            }
        }
        out
    }

    /// 템플릿이 `{N}` 으로 시작하면 N.
    pub(crate) fn first_ref(&self) -> Option<usize> {
        match self.parts.first() {
            Some(Part::Ref(n)) => Some(*n),
            _ => None,
        }
    }

    /// 템플릿이 `{N}` 으로 끝나면 N.
    pub(crate) fn last_ref(&self) -> Option<usize> {
        match self.parts.last() {
            Some(Part::Ref(n)) => Some(*n),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(s: &str) -> Pattern {
        parse_rust_fmt(s).unwrap()
    }

    #[test]
    fn escaped_braces_are_literals() {
        let p = pat("set {{x}} to {}");
        assert_eq!(p.groups(), 1);
        assert_eq!(p.captures("set {x} to 5"), Some(vec!["5"]));
        assert_eq!(p.captures("set x to 5"), None);
    }

    #[test]
    fn placeholder_forms_are_one_group_each() {
        for f in [
            "a {} b",
            "a {0} b",
            "a {name} b",
            "a {:?} b",
            "a {:#?} b",
            "a {:#} b",
            "a {x:>3} b",
            "a {:.1} b",
            "a {:width$} b",
            "a {r#type} b",
        ] {
            let p = pat(f);
            assert_eq!(p.groups(), 1, "{f}");
            assert_eq!(p.captures("a ZZ b"), Some(vec!["ZZ"]), "{f}");
        }
    }

    #[test]
    fn unmatched_close_brace_is_error() {
        assert!(parse_rust_fmt("oops } here").is_err());
        assert!(parse_rust_fmt("oops {{ } here").is_err());
    }

    #[test]
    fn unclosed_or_bad_placeholder_is_error() {
        assert!(parse_rust_fmt("oops { here").is_err());
        assert!(parse_rust_fmt("oops {a b} here").is_err());
        assert!(parse_rust_fmt("oops {{{ here").is_err());
    }

    #[test]
    fn adjacent_style_placeholders_merge() {
        let p = pat("hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms");
        assert_eq!(p.groups(), 1);
        let c = p.captures("hotpatched in \x1b[1m12\x1b[0mms").unwrap();
        assert_eq!(c, vec!["\x1b[1m12\x1b[0m"]);
        // literal between placeholders keeps them apart
        assert_eq!(pat("{} and {}").groups(), 2);
        assert_eq!(pat("{}{{}}{}").groups(), 2);
    }

    #[test]
    fn literal_alpha_counts_only_letters() {
        assert_eq!(pat("{}").literal_alpha(), 0);
        assert_eq!(pat("{}: {}").literal_alpha(), 0);
        assert_eq!(pat("[{}] {}").literal_alpha(), 0);
        assert_eq!(pat("Serving app at {}").literal_alpha(), 12);
        assert_eq!(pat("한글 {}").literal_alpha(), 2);
        assert!(pat("ab {}").literal_alpha() < MIN_LITERAL_ALPHA);
    }

    #[test]
    fn captures_anchor_start_and_end() {
        let p = pat("Failed to read {} from {}");
        assert_eq!(p.captures("Failed to read a from b"), Some(vec!["a", "b"]));
        assert_eq!(p.captures("xx Failed to read a from b"), None);
        let q = pat("took {}ms");
        assert_eq!(q.captures("took 5ms"), Some(vec!["5"]));
        assert_eq!(q.captures("took 5ms later"), None);
    }

    #[test]
    fn captures_allow_empty_group() {
        let p = pat("a: {} :b");
        assert_eq!(p.captures("a:  :b"), Some(vec![""]));
        let q = pat("tail {}");
        assert_eq!(q.captures("tail "), Some(vec![""]));
    }

    #[test]
    fn captures_do_not_overlap_prefix_and_suffix() {
        let p = pat("ab{}ba");
        assert_eq!(p.captures("aba"), None);
        assert_eq!(p.captures("abba"), Some(vec![""]));
    }

    #[test]
    fn middle_literal_is_found_leftmost() {
        let p = pat("{} -> {} -> end");
        assert_eq!(p.captures("a -> b -> c -> end"), Some(vec!["a", "b -> c"]));
    }

    #[test]
    fn multibyte_and_emoji_literals() {
        let p = pat("🚀 시작 {} 완료 ✅");
        assert_eq!(p.captures("🚀 시작 앱 완료 ✅"), Some(vec!["앱"]));
        assert_eq!(p.captures("🚀 시작 앱 완료"), None);
    }

    #[test]
    fn template_reorders_and_repeats() {
        let t = parse_ko_template("{2}에서 {1}을(를) 읽지 못했습니다 ({1})", 2).unwrap();
        assert_eq!(t.render(&["a", "b"]), "b에서 a을(를) 읽지 못했습니다 (a)");
    }

    #[test]
    fn template_out_of_range_is_error() {
        assert!(parse_ko_template("{3}", 2).is_err());
        assert!(parse_ko_template("{0}", 1).is_err());
        assert!(parse_ko_template("{x}", 1).is_err());
        assert!(parse_ko_template("{1", 1).is_err());
    }

    #[test]
    fn template_unused_group_is_error() {
        assert!(parse_ko_template("{1}", 2).is_err());
        assert!(parse_ko_template("없음", 1).is_err());
    }

    #[test]
    fn template_escapes_braces() {
        let t = parse_ko_template("{{{1}}}", 1).unwrap();
        assert_eq!(t.render(&["v"]), "{v}");
        assert!(parse_ko_template("{1} }", 1).is_err());
    }

    #[test]
    fn template_preserves_ansi_values_bytes() {
        let t = parse_ko_template("{1}ms 만에 완료", 1).unwrap();
        assert_eq!(
            t.render(&["\x1b[1;32m7\x1b[0m"]),
            "\x1b[1;32m7\x1b[0mms 만에 완료"
        );
        assert_eq!(t.first_ref(), Some(1));
        assert_eq!(t.last_ref(), None);
    }

    #[test]
    fn style_only_groups_match_only_empty_or_sgr() {
        let p = pat("{LINK_STYLE}Setup{LINK_STYLE:#}");
        assert!(p.captures("Setup").is_some());
        assert!(p.captures("\x1b[4;34mSetup\x1b[0m").is_some());
        assert!(p.captures("\x1b[1m\x1b[4mSetup\x1b[0m").is_some());
        assert!(
            p.captures("Setup of the Android toolchain failed")
                .is_none()
        );
        assert!(p.captures("Could not connect to Devtools server").is_none());
        let d = pat("{LINK_STYLE}Devtools{LINK_STYLE:#}");
        assert!(d.captures("Could not connect to Devtools server").is_none());
        assert!(d.captures("\x1b[34mDevtools\x1b[0m").is_some());
        let e = pat("{ERROR_STYLE}Build failed{ERROR_STYLE:#}: {}");
        assert!(e.captures("Build failed: boom").is_some());
        assert!(e.captures("\x1b[31mBuild failed\x1b[0m: boom").is_some());
        assert!(e.captures("The Build failed: boom").is_none());
        assert!(e.captures("Build failed badly: boom").is_none());
    }

    #[test]
    fn open_end_ignores_style_only_groups() {
        assert!(!pat("{LINK_STYLE}Setup{LINK_STYLE:#}").is_open());
        assert!(!pat("Hello {HINT_STYLE}({x}){HINT_STYLE:#}").is_open());
        assert!(pat("{ERROR_STYLE}Build failed{ERROR_STYLE:#}: {}").is_open());
        assert!(pat("from {src:?}").is_open());
        assert!(pat("{} changed").is_open());
        assert!(!pat("took {} ms").is_open());
        assert!(pat("hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}").is_open());
    }

    #[test]
    fn style_check_failure_backtracks_to_next_literal_position() {
        let p = pat(
            "Full rebuild: {NOTE_STYLE}{subject}{NOTE_STYLE:#} {detail} {HINT_STYLE}({display_file}){HINT_STYLE:#}",
        );
        let styled = "Full rebuild: \x1b[32mCargo.toml [dependencies]\x1b[0m changed \x1b[90m(Cargo.toml)\x1b[0m";
        let plain = "Full rebuild: Cargo.toml [dependencies] changed (Cargo.toml)";
        for line in [styled, plain] {
            let caps = p
                .captures(line)
                .unwrap_or_else(|| panic!("no match: {line:?}"));
            assert_eq!(caps.len(), 5, "{line:?}");
        }
        // 가장 왼쪽 해: 첫 값 묶음이 가능한 한 짧고, 스타일 묶음은 SGR 만 잡는다
        let caps = p.captures(styled).unwrap();
        assert_eq!(caps[0], "\x1b[32mCargo.toml");
        assert_eq!(caps[1], "[dependencies]\x1b[0m changed");
        assert_eq!(caps[2], "\x1b[90m");
        assert_eq!(caps[3], "Cargo.toml");
        assert_eq!(caps[4], "\x1b[0m");
        for line in [styled, plain] {
            let mut want = Vec::new();
            assert!(reference(&p.segs, line, 0, &mut want));
            assert_eq!(p.captures(line), Some(want));
        }
        // 후속 조각 실패로 되돌아가는 일반 묶음
        let q = pat("{} -> {} !");
        assert_eq!(q.captures("a -> b -> c !"), Some(vec!["a", "b -> c"]));
        // 시도 횟수 상한: 해가 없는 병적 입력은 None 으로 끝난다
        // (접두·접미는 맞고 가운데 리터럴 조합만 폭발하며, 가운데 "c" 가 없어 해가 없다)
        let r = pat("{}a{}a{}a{}a{}c{}b");
        let input = "a".repeat(400) + "b";
        let t = std::time::Instant::now();
        assert_eq!(r.captures(&input), None);
        assert!(t.elapsed() < std::time::Duration::from_secs(1));
    }

    /// 정규식 의미(스타일 전용 묶음 `(?:\x1b\[[0-9;:]*m)*`, 일반 묶음 `.*?`)를 글자 단위로 흉내 낸 기준 구현.
    fn reference<'a>(segs: &[Seg], s: &'a str, pos: usize, caps: &mut Vec<&'a str>) -> bool {
        fn sgr(s: &str) -> bool {
            let b: Vec<char> = s.chars().collect();
            let mut i = 0;
            while i < b.len() {
                if b[i] != '\x1b' || b.get(i + 1) != Some(&'[') {
                    return false;
                }
                i += 2;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == ';' || b[i] == ':') {
                    i += 1;
                }
                if b.get(i) != Some(&'m') {
                    return false;
                }
                i += 1;
            }
            true
        }
        match segs.split_first() {
            None => pos == s.len(),
            Some((Seg::Lit(l), rest)) => {
                s[pos..].starts_with(l.as_str()) && reference(rest, s, pos + l.len(), caps)
            }
            Some((Seg::Group { style_only }, rest)) => {
                let ends: Vec<usize> = s[pos..]
                    .char_indices()
                    .map(|(i, _)| pos + i)
                    .chain([s.len()])
                    .collect();
                for end in ends {
                    let cap = &s[pos..end];
                    if *style_only && !sgr(cap) {
                        continue;
                    }
                    caps.push(cap);
                    if reference(rest, s, end, caps) {
                        return true;
                    }
                    caps.pop();
                }
                false
            }
        }
    }

    #[test]
    fn captures_match_reference_backtracking_on_random_inputs() {
        // 결정적 xorshift 시드
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move |n: usize| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % n as u64) as usize
        };
        let alphabet = ["a", "b", " ", ":", "\x1b[", "m", "0", ";", "1", "é"];
        let lits = ["a", "b", " ", ": ", "ab", "\x1b[0m", "m", "é", "a b"];
        let mut matched = 0;
        for _ in 0..6000 {
            let mut fmt = String::new();
            let n = 1 + next(5);
            for _ in 0..n {
                match next(4) {
                    0 => fmt.push_str("{}"),
                    1 => fmt.push_str("{X_STYLE}"),
                    _ => fmt.push_str(lits[next(lits.len())]),
                }
                if next(3) == 0 {
                    fmt.push_str(lits[next(lits.len())]);
                }
            }
            let p = parse_rust_fmt(&fmt).unwrap();
            let mut s = String::new();
            for _ in 0..next(14) {
                s.push_str(alphabet[next(alphabet.len())]);
            }
            let mut want = Vec::new();
            let ok = reference(&p.segs, &s, 0, &mut want);
            let got = p.captures(&s);
            if ok {
                matched += 1;
                assert_eq!(got, Some(want), "fmt={fmt:?} s={s:?}");
            } else {
                assert_eq!(got, None, "fmt={fmt:?} s={s:?}");
            }
        }
        assert!(matched > 300, "too few matching cases: {matched}");
    }
}
