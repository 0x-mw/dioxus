//! 개발용 웹 HTML 한국어화: 웹 런타임(packages/web)이 영어로 정해 부르는 재빌드 토스트를 dx 가 내주는 HTML 에서 번역한다.
//!
//! 웹 런타임은 건드리지 않고, 개발 HTML 에 `window.showDXToast`/`window.scheduleDXToast` 를 감싸는 `<script>` 를 끼워
//! 정확히 일치하는 영어 문구만 바꾼다. 번역 훅이 꺼져 있으면(En, 기계 모드) HTML 은 바이트 그대로다.

use std::collections::BTreeMap;

use super::Table;

/// 웹 런타임 `devtools.rs` 가 토스트로 보이는 영어 문구(번역표의 `[[text]]` 키).
const TOAST_KEYS: [&str; 10] = [
    "Hot-patch success!",
    "Your app is being rebuilt.",
    "A non-hot-reloadable change occurred and we must rebuild.",
    "Hot-patching app...",
    "Hot-patching modified Rust code.",
    "Oops! The build failed.",
    "We tried to rebuild your app, but something went wrong.",
    "Successfully rebuilt.",
    "Your app was rebuilt successfully and without error.",
    "App panicked! See console for details.",
];

/// 값이 끼는 토스트 문구(번역표의 `[[fmt]]` 키). 웹 런타임 `lib.rs` 의 핫 패치 성공 알림.
const TOAST_FMT_KEYS: [&str; 1] = ["App successfully patched in {} ms"];

/// 값 자리를 표시하는 임시 문자열(번호 i). 번역표를 한 번 통과시켜 ko 틀의 `{N}` 위치를 알아낸다.
fn sentinel(i: usize) -> String {
    format!("\u{1}{i}\u{2}")
}

/// JS 정규식 특수문자를 이스케이프한다.
fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\^$.*+?()[]{}|/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// en 서식 문자열(`{}` 값 자리)을 [정규식, 치환 템플릿] 으로 바꾼다. 번역이 없거나 같으면 None.
fn fmt_pattern(table: &Table, en: &str) -> Option<[String; 2]> {
    let pieces: Vec<&str> = en.split("{}").collect();
    let groups = pieces.len() - 1;
    let mut sample = String::from(pieces[0]);
    for (i, piece) in pieces.iter().enumerate().skip(1) {
        sample.push_str(&sentinel(i));
        sample.push_str(piece);
    }
    let ko = table.tr_line(&sample).into_owned();
    if ko == sample {
        return None;
    }
    // ko 안의 임시 문자열을 `$N` 으로, 나머지 `$` 는 `$$` 로 바꾼다.
    let mut tpl = String::new();
    let mut rest = ko.as_str();
    'outer: while !rest.is_empty() {
        for i in 1..=groups {
            let sen = sentinel(i);
            if let Some(r) = rest.strip_prefix(sen.as_str()) {
                tpl.push_str(&format!("${i}"));
                rest = r;
                continue 'outer;
            }
        }
        let c = rest.chars().next()?;
        if c == '$' {
            tpl.push_str("$$");
        } else {
            tpl.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    if tpl.contains('\u{1}') || tpl.contains('\u{2}') {
        return None;
    }
    let re = format!(
        "^{}$",
        pieces
            .iter()
            .map(|p| escape_regex(p))
            .collect::<Vec<_>>()
            .join("(.*?)")
    );
    Some([re, tpl])
}

/// dev.index.html 에 들어 있는 숨은 기본 토스트 문구(h3, p).
const DEFAULT_H3: &str = "Your app is being rebuilt.";
const DEFAULT_P: &str = "A non-hot-reloadable change occurred and we must rebuild.";

const WRAPPER_JS: &str = r#"(function(){var t=function(s){if(typeof s!=="string")return s;if(Object.prototype.hasOwnProperty.call(M,s))return M[s];for(var i=0;i<P.length;i++){var r=new RegExp(P[i][0]);if(r.test(s))return s.replace(r,P[i][1])}return s};var wrap=function(){["showDXToast","scheduleDXToast"].forEach(function(n){var f=window[n];if(typeof f!=="function"||f.__dxKo)return;var w=function(h,m){var a=Array.prototype.slice.call(arguments);a[0]=t(h);a[1]=t(m);return f.apply(this,a)};w.__dxKo=true;window[n]=w})};wrap();document.addEventListener("DOMContentLoaded",wrap)})();"#;

/// 개발용 HTML 에 토스트 번역을 끼운다. 번역 훅이 꺼져 있으면 아무것도 하지 않는다.
pub(crate) fn localize_dev_html(html: &mut String) {
    if !super::hooks_active() {
        return;
    }
    localize_dev_html_with(html, super::hook_table());
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `</script>`·`<!--` 가 스크립트를 끊지 못하도록 JSON 안의 `<` 뒤를 이스케이프한다.
fn escape_script_json(json: &str) -> String {
    json.replace("</", "<\\/").replace("<!--", "<\\!--")
}

/// 표를 인자로 받는 순수 함수. 표가 None 이거나 번역이 하나도 없으면 HTML 을 바꾸지 않는다.
pub(crate) fn localize_dev_html_with(html: &mut String, table: Option<&Table>) {
    let Some(table) = table else { return };
    let mut map: BTreeMap<&str, &str> = BTreeMap::new();
    for key in TOAST_KEYS {
        if let Some(ko) = table.text(key).filter(|ko| !ko.is_empty()) {
            map.insert(key, ko);
        }
    }
    let pats: Vec<[String; 2]> = TOAST_FMT_KEYS
        .iter()
        .filter_map(|en| fmt_pattern(table, en))
        .collect();
    if map.is_empty() && pats.is_empty() {
        return;
    }
    let (Ok(json), Ok(pjson)) = (serde_json::to_string(&map), serde_json::to_string(&pats)) else {
        return;
    };
    let script = format!(
        "<script>var M={};var P={};{}</script>\n",
        escape_script_json(&json),
        escape_script_json(&pjson),
        WRAPPER_JS
    );
    // 기본 토스트 문구: 정확히 일치할 때만 치환한다.
    for (tag, en) in [("h3", DEFAULT_H3), ("p", DEFAULT_P)] {
        if let Some(ko) = map.get(en) {
            let from = format!(">{en}</{tag}>");
            let to = format!(">{}</{tag}>", escape_html(ko));
            *html = html.replace(&from, &to);
        }
    }
    let at = html
        .find("</head>")
        .or_else(|| html.find("</body>"))
        .unwrap_or(html.len());
    html.insert_str(at, &script);
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = "<html><head><title>x</title></head><body><h3 id=\"__dx-toast-text\" class=\"c\">Your app is being rebuilt.</h3><p id=\"__dx-toast-msg\">A non-hot-reloadable change occurred and we must rebuild.</p></body></html>";

    fn table(src: &str) -> Table {
        let (t, errs) = Table::parse(src);
        assert!(errs.is_empty(), "{errs:?}");
        t
    }

    fn fake() -> Table {
        table(
            r#"
[[text]]
en = "Your app is being rebuilt."
ko = "앱을 다시 빌드하는 중입니다."

[[text]]
en = "A non-hot-reloadable change occurred and we must rebuild."
ko = "핫 리로드할 수 없는 변경이 있어 다시 빌드합니다."
"#,
        )
    }

    #[test]
    fn none_table_is_identity() {
        let mut h = HTML.to_string();
        localize_dev_html_with(&mut h, None);
        assert_eq!(h, HTML);
    }

    #[test]
    fn table_without_toast_keys_is_identity() {
        let t = table("[[text]]\nen = \"Other\"\nko = \"다른\"\n");
        let mut h = HTML.to_string();
        localize_dev_html_with(&mut h, Some(&t));
        assert_eq!(h, HTML);
    }

    #[test]
    fn injects_script_before_head_and_translates_defaults() {
        let mut h = HTML.to_string();
        localize_dev_html_with(&mut h, Some(&fake()));
        assert_eq!(h.matches("<script>").count(), 1);
        let s = h.find("<script>").unwrap();
        assert!(s < h.find("</head>").unwrap());
        assert!(h.contains("\"Your app is being rebuilt.\":\"앱을 다시 빌드하는 중입니다.\""));
        assert!(h.contains(">앱을 다시 빌드하는 중입니다.</h3>"));
        assert!(h.contains(">핫 리로드할 수 없는 변경이 있어 다시 빌드합니다.</p>"));
        assert!(h.contains("showDXToast") && h.contains("scheduleDXToast"));
    }

    #[test]
    fn without_head_uses_body_then_end() {
        let t = fake();
        let mut h = "<body>hi</body>".to_string();
        localize_dev_html_with(&mut h, Some(&t));
        assert!(h.find("<script>").unwrap() < h.find("</body>").unwrap());
        let mut h = "plain".to_string();
        localize_dev_html_with(&mut h, Some(&t));
        assert!(h.starts_with("plain<script>") && h.ends_with("</script>\n"));
    }

    #[test]
    fn untranslated_default_text_is_left_alone() {
        let mut h = "<head></head><h3>Something else</h3>".to_string();
        localize_dev_html_with(&mut h, Some(&fake()));
        assert!(h.contains("<h3>Something else</h3>"));
    }

    #[test]
    fn hostile_translation_does_not_break_script() {
        let t = table("[[text]]\nen = \"Hot-patching app...\"\nko = \"a\\\"b</script><!--c\"\n");
        let mut h = "<head></head>".to_string();
        localize_dev_html_with(&mut h, Some(&t));
        assert_eq!(h.matches("</script>").count(), 1);
        assert_eq!(h.matches("<script>").count(), 1);
        assert!(!h.contains("<!--"));
        assert!(h.contains(r#"a\"b<\/script><\!--c"#));
    }

    const PATCH_FMT: &str = "[[fmt]]\nen = \"App successfully patched in {} ms\"\nko = \"앱을 {1}ms 만에 핫 패치했습니다\"\n";

    #[test]
    fn fmt_pattern_goes_into_script() {
        let mut h = HTML.to_string();
        localize_dev_html_with(&mut h, Some(&table(PATCH_FMT)));
        assert_eq!(h.matches("<script>").count(), 1);
        assert!(h.contains(r#"var P=[["^App successfully patched in (.*?) ms$","앱을 $1ms 만에 핫 패치했습니다"]];"#));
        assert!(h.contains("var M={};"));
    }

    #[test]
    fn fmt_pattern_escapes_regex_and_dollar() {
        let t = table("[[fmt]]\nen = \"a.b (c) [d]? *+ {} $x\"\nko = \"값 {1}$ 와 $1\"\n");
        let [re, tpl] = fmt_pattern(&t, "a.b (c) [d]? *+ {} $x").unwrap();
        assert_eq!(re, r"^a\.b \(c\) \[d\]\? \*\+ (.*?) \$x$");
        assert_eq!(tpl, "값 $1$$ 와 $$1");
    }

    #[test]
    fn fmt_pattern_absent_without_translation() {
        assert!(fmt_pattern(&fake(), "App successfully patched in {} ms").is_none());
        let mut h = HTML.to_string();
        localize_dev_html_with(&mut h, Some(&fake()));
        assert!(h.contains("var P=[];"));
    }

    #[test]
    fn public_fn_is_identity_in_tests_without_injected_table() {
        let mut h = HTML.to_string();
        localize_dev_html(&mut h);
        assert_eq!(h, HTML);
    }
}
