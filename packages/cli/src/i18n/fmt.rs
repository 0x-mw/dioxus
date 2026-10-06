//! 로그·TUI 번역 보조: tracing 필드 포매터 래퍼, 재발행 가드, 꼬리 필드 보존 번역.
//!
//! 번역은 영어 메시지를 화면에 쓰기 직전에만 한다. 로그 파일·텔레메트리·JSON 은 원문을 쓴다.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt::Debug;

use tracing::field::{Field, Visit};
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::FormatFields;
use tracing_subscriber::fmt::format::Writer;

use crate::TraceSrc;

thread_local! {
    /// 지금 포매팅 중인 이벤트의 `message` 를 번역해도 되는가.
    static TRANSLATE_MESSAGE: Cell<bool> = const { Cell::new(false) };
    /// `untranslated` 가드 깊이.
    static UNTRANSLATED_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// 현재 이벤트의 `message` 필드를 번역해야 하는가(`LocalizedFields` 안에서만 true 가 된다).
pub(crate) fn translate_message() -> bool {
    TRANSLATE_MESSAGE.with(|c| c.get())
}

struct DepthGuard;

impl DepthGuard {
    fn new() -> Self {
        UNTRANSLATED_DEPTH.with(|c| c.set(c.get() + 1));
        DepthGuard
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        UNTRANSLATED_DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// 안에서 내보내는 tracing 이벤트는 번역하지 않는다(App·Cargo 출력을 출처 정보 없이 재발행할 때 쓴다).
/// 패닉이 나도 깊이는 복원된다.
pub(crate) fn untranslated<R>(f: impl FnOnce() -> R) -> R {
    let _guard = DepthGuard::new();
    f()
}

fn untranslated_depth() -> usize {
    UNTRANSLATED_DEPTH.with(|c| c.get())
}

/// 번역 여부 판정(순수 함수). `src` 는 `dx_src` 필드의 문자열 값.
pub(crate) fn should_translate(
    hooks_active: bool,
    guard_depth: usize,
    has_json: bool,
    src: Option<&str>,
) -> bool {
    if !hooks_active || guard_depth > 0 || has_json {
        return false;
    }
    match src {
        None => true,
        Some(s) => !matches!(
            TraceSrc::from(s.to_string()),
            TraceSrc::Cargo | TraceSrc::App(_)
        ),
    }
}

/// 이벤트 필드를 한 번 훑어 `json` 필드 유무와 `dx_src` 값을 모은다.
#[derive(Default)]
struct Scan {
    has_json: bool,
    src: Option<String>,
}

impl Visit for Scan {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        match field.name() {
            "json" => self.has_json = true,
            "dx_src" => self.src = Some(format!("{value:?}")),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "json" => self.has_json = true,
            "dx_src" => self.src = Some(value.to_string()),
            _ => {}
        }
    }
}

/// 내부 필드 포매터를 감싸 번역 조건이 맞을 때만 `translate_message()` 를 켠다.
pub(crate) struct LocalizedFields<F>(pub F);

impl<'writer, F> FormatFields<'writer> for LocalizedFields<F>
where
    F: FormatFields<'writer>,
{
    fn format_fields<R: RecordFields>(
        &self,
        writer: Writer<'writer>,
        fields: R,
    ) -> std::fmt::Result {
        // En·기계 모드에서는 필드를 훑지 않고 그대로 위임한다.
        if !super::hooks_active() {
            return self.0.format_fields(writer, fields);
        }

        let mut scan = Scan::default();
        fields.record(&mut scan);
        let on = should_translate(
            true,
            untranslated_depth(),
            scan.has_json,
            scan.src.as_deref(),
        );

        let prev = TRANSLATE_MESSAGE.with(|c| c.replace(on));
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                TRANSLATE_MESSAGE.with(|c| c.set(self.0));
            }
        }
        let _restore = Restore(prev);
        self.0.format_fields(writer, fields)
    }
}

/// 마지막 줄 끝의 ` key=value …` 꼬리(와 끝 공백)를 떼고 `f` 로 번역한 뒤 다시 붙인다.
/// `CollectVisitor::pretty` 는 메시지 뒤와 각 필드 뒤에 공백을 붙이므로 꼬리에는 그 공백이 들어간다.
pub(crate) fn translate_with_tail<'a>(
    text: &'a str,
    f: impl Fn(&str) -> Cow<'_, str>,
) -> Cow<'a, str> {
    // 본문이 `x=y` 로 끝나는 메시지를 보호하려고 마지막 줄 전체로 먼저 시도한다.
    let last_start = text.trim_end_matches(' ').rfind('\n').map_or(0, |i| i + 1);
    match f(&text[last_start..]) {
        Cow::Borrowed(b) if b.len() == text.len() - last_start => {}
        out => {
            let mut s = f(&text[..last_start]).into_owned();
            s.push_str(&out);
            return Cow::Owned(s);
        }
    }
    let body_end = text.trim_end_matches(' ').len();
    let mut head_end = body_end;
    // 뒤에서부터 ` key=value` 토큰을 떼어 낸다.
    loop {
        let seg = &text[..head_end];
        let line_start = seg.rfind('\n').map_or(0, |i| i + 1);
        let Some(sp) = seg[line_start..].rfind(' ') else {
            break;
        };
        let tok = &seg[line_start + sp + 1..];
        if is_field_token(tok) {
            head_end = line_start + sp;
        } else {
            break;
        }
    }
    let head = &text[..head_end];
    let tail = &text[head_end..];
    match f(head) {
        Cow::Borrowed(b) if b.len() == head.len() => Cow::Borrowed(text),
        out => {
            let mut s = out.into_owned();
            s.push_str(tail);
            Cow::Owned(s)
        }
    }
}

/// `[a-z_]+=\S*` 인가.
fn is_field_token(tok: &str) -> bool {
    let Some((k, v)) = tok.split_once('=') else {
        return false;
    };
    !k.is_empty()
        && k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        && !v.contains(char::is_whitespace)
}

/// 꼬리 필드를 보존하는 `tr_text`(TUI 로그용).
pub(crate) fn tr_text_tail(text: &str) -> Cow<'_, str> {
    if !super::hooks_active() {
        return Cow::Borrowed(text);
    }
    translate_with_tail(text, super::tr_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Table, ko_source, set_test_table};
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::fmt::format;
    use tracing_subscriber::prelude::*;

    const TABLE: &str = r#"
[[fmt]]
en = "Serving app at {}"
ko = "앱 주소: {1}"

[[text]]
en = "Failed"
ko = "실패"
"#;

    fn table() -> &'static Table {
        let (t, errs) = Table::parse(TABLE);
        assert!(errs.is_empty(), "{errs:?}");
        Box::leak(Box::new(t))
    }

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Buf {
        type Writer = Buf;
        fn make_writer(&'a self) -> Buf {
            self.clone()
        }
    }

    /// logging.rs 와 같은 모양의 필드 포매터(번역 분기 포함)로 `f` 의 출력을 캡처한다.
    fn capture(f: impl FnOnce()) -> String {
        let buf = Buf::default();
        let layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .without_time()
            .with_ansi(false)
            .with_writer(buf.clone())
            .fmt_fields(LocalizedFields(
                format::debug_fn(|writer, field, value| {
                    if field.name() == "message" && translate_message() {
                        return write!(writer, "{}", tr_text_tail(&format!("{value:?}")));
                    }
                    if field.name() == "message" {
                        return write!(writer, "{value:?}");
                    }
                    write!(writer, "{}={value:?}", field.name())
                })
                .delimited(" "),
            ));
        let sub = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(sub, f);
        String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
    }

    fn with_table<R>(f: impl FnOnce() -> R) -> R {
        set_test_table(Some(table()));
        let r = f();
        set_test_table(None);
        r
    }

    #[test]
    fn translates_plain_event() {
        let out = with_table(|| capture(|| tracing::info!("Serving app at x")));
        assert!(out.contains("앱 주소: x"), "{out}");
        assert!(!out.contains("Serving app"), "{out}");
    }

    #[test]
    fn cargo_and_app_sources_stay_english() {
        let out = with_table(|| {
            capture(|| {
                tracing::warn!(dx_src = ?TraceSrc::Cargo, "Serving app at x");
                tracing::info!(
                    dx_src = ?TraceSrc::App(crate::BundleFormat::Web),
                    "Serving app at y"
                );
                tracing::info!(dx_src = ?TraceSrc::Dev, "Serving app at z");
            })
        });
        assert!(out.contains("Serving app at x"), "{out}");
        assert!(out.contains("Serving app at y"), "{out}");
        assert!(out.contains("앱 주소: z"), "{out}");
    }

    #[test]
    fn json_field_event_stays_english() {
        let out =
            with_table(|| capture(|| tracing::info!(json = "x", message = "Serving app at x")));
        assert!(out.contains("Serving app at x"), "{out}");
        assert!(!out.contains("앱 주소"), "{out}");
    }

    #[test]
    fn untranslated_guard_blocks_and_restores() {
        let out = with_table(|| {
            capture(|| {
                untranslated(|| tracing::info!("Failed"));
                tracing::info!("Failed");
            })
        });
        let mut lines = out.lines();
        assert!(lines.next().unwrap().contains("Failed"), "{out}");
        assert!(lines.next().unwrap().contains("실패"), "{out}");
    }

    #[test]
    fn guard_restores_after_panic() {
        let r = std::panic::catch_unwind(|| untranslated(|| panic!("boom")));
        assert!(r.is_err());
        assert_eq!(untranslated_depth(), 0);
    }

    #[test]
    fn nested_guard_depth() {
        untranslated(|| {
            assert_eq!(untranslated_depth(), 1);
            untranslated(|| assert_eq!(untranslated_depth(), 2));
            assert_eq!(untranslated_depth(), 1);
        });
        assert_eq!(untranslated_depth(), 0);
    }

    #[test]
    fn en_mode_passes_through() {
        // 주입 표가 없으면(=En) 번역하지 않는다.
        let out = capture(|| tracing::info!("Serving app at x"));
        assert!(out.contains("Serving app at x"), "{out}");
    }

    #[test]
    fn should_translate_table() {
        let cases: [(bool, usize, bool, Option<&str>, bool); 11] = [
            (true, 0, false, None, true),
            (false, 0, false, None, false),
            (true, 1, false, None, false),
            (true, 0, true, None, false),
            (true, 0, false, Some("cargo"), false),
            (true, 0, false, Some("web"), false),
            (true, 0, false, Some("dev"), true),
            (true, 0, false, Some("bld"), true),
            (true, 0, false, Some("n/a"), true),
            (true, 0, false, Some("bundle"), true),
            (false, 0, true, Some("cargo"), false),
        ];
        for (a, d, j, s, want) in cases {
            assert_eq!(should_translate(a, d, j, s), want, "{a} {d} {j} {s:?}");
        }
    }

    #[test]
    fn tail_fields_are_split_and_reattached() {
        let t = table();
        let tr = |s: &str| translate_with_tail(s, |h| t.tr_text(h)).into_owned();
        // pretty(): 메시지 뒤와 각 필드 뒤에 공백
        assert_eq!(tr("Serving app at x "), "앱 주소: x ");
        assert_eq!(
            tr("Serving app at x a=1 b_c=\"d\" "),
            "앱 주소: x a=1 b_c=\"d\" "
        );
        assert_eq!(tr("Failed k=v"), "실패 k=v");
        // 여러 줄: 마지막 줄 꼬리만 뗀다
        assert_eq!(tr("Failed\nFailed a=1 "), "실패\n실패 a=1 ");
        // 꼬리가 없으면 그대로(번역 없음 → 원문 동일)
        assert_eq!(tr("nothing here "), "nothing here ");
        // 전체가 key=value 한 토큰이면 떼지 않는다
        assert_eq!(tr("a=b"), "a=b");
        // 본문이 `x=y` 로 끝나는 메시지는 원문 전체로 먼저 번역된다
        let t2 = {
            let (t, e) = Table::parse("[[text]]\nen = \"Mode x=y\"\nko = \"모드 x=y\"\n");
            assert!(e.is_empty(), "{e:?}");
            &*Box::leak(Box::new(t))
        };
        assert_eq!(
            translate_with_tail("Mode x=y ", |h| t2.tr_text(h)),
            "모드 x=y "
        );
        assert!(matches!(
            translate_with_tail("zzz q=1 ", |h| t.tr_text(h)),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn noninteractive_message_keeps_reemitted_tail_fields() {
        let out = with_table(|| capture(|| tracing::info!("Serving app at x a=1 b=\"c\"")));
        assert!(out.contains("앱 주소: x a=1 b=\"c\""), "{out}");
        assert!(!out.contains("Serving app"), "{out}");
    }

    /// serve/output.rs 의 t(/t_pad( 인자 리터럴을 뽑는다.
    fn literals_in(src: &str, call: &str) -> Vec<String> {
        let mut out = vec![];
        let mut rest = src;
        while let Some(i) = rest.find(call) {
            let before = rest[..i].chars().last();
            rest = &rest[i + call.len()..];
            // 식별자 끝(예: format!(, split_once()) 은 제외
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                continue;
            }
            let r = rest.trim_start();
            if let Some(r) = r.strip_prefix('"') {
                let mut s = String::new();
                let mut it = r.chars();
                while let Some(c) = it.next() {
                    match c {
                        '\\' => match it.next() {
                            Some('"') => s.push('"'),
                            Some('\\') => s.push('\\'),
                            Some('n') => s.push('\n'),
                            Some(o) => {
                                s.push('\\');
                                s.push(o)
                            }
                            None => break,
                        },
                        '"' => break,
                        c => s.push(c),
                    }
                }
                out.push(s);
            }
        }
        out
    }

    #[test]
    fn output_rs_keys_exist_in_ko_table() {
        let full = include_str!("../serve/output.rs");
        // 테스트 모듈 이후는 보지 않는다(없으면 전체).
        let src = full.split("#[cfg(test)]").next().unwrap_or(full);
        let mut keys: Vec<String> = literals_in(src, "t(");
        keys.extend(literals_in(src, "t_pad("));
        let keys: Vec<String> = keys.into_iter().map(|k| k.trim().to_string()).collect();
        assert!(!keys.is_empty(), "t(/t_pad( 인자를 하나도 찾지 못함");
        let (table, _) = Table::parse(ko_source());
        if table.is_empty() {
            println!("skip: ko.toml 표가 비어 있음");
            return;
        }
        let missing: Vec<&String> = keys
            .iter()
            .filter(|k| !k.is_empty() && table.text(k).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "ko.toml [[text]] 에 없는 키: {missing:?}"
        );
        // 단축키 문구("r: rebuild the app")는 ko 도 같은 "키: " 접두로 시작해야 한다.
        for k in keys.iter().filter(|k| k.contains(": ")) {
            let (pre, _) = k.split_once(": ").unwrap();
            let ko = table.text(k).unwrap_or_else(|| panic!("키 없음: {k:?}"));
            assert!(
                ko.starts_with(&format!("{pre}: ")),
                "단축키 접두가 다름: {k:?} -> {ko:?}"
            );
        }
    }
}
