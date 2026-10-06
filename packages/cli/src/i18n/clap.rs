//! clap 도움말·오류 한국어화.
//!
//! 정상 실행은 영어 트리로 파싱한 결과를 그대로 쓴다(번역표 미적재, 파싱 결과 동등성 구성상 보장).
//! 도움말·오류(`Err`)일 때만 번역 트리로 다시 파싱해 렌더하고, clap 이 만드는 내장 문구는 렌더 결과를 줄 단위로 후처리한다.

use std::borrow::Cow;
use std::cell::Cell;
use std::ffi::OsString;
use std::io::Write;

use clap::builder::StyledStr;
use clap::error::ErrorKind;
use clap::{Command, CommandFactory, FromArgMatches};

use super::Table;
use crate::Cli;
use crate::shell_completions::GENERATING_COMPLETIONS;

thread_local! {
    /// 지연(defer) 하위명령 번역에 쓸 표. parse_cli 가 번역 트리로 다시 파싱하기 직전에만 채운다.
    static DEFERRED_TABLE: Cell<Option<&'static Table>> = const { Cell::new(None) };
}

/// clap 이 렌더 때 만드는 내장 문구(`[[text]]` 키).
#[allow(dead_code)]
pub(crate) const CLAP_BUILTIN_TEXT: &[&str] = &[
    "Usage:",
    "Options:",
    "Arguments:",
    "Commands:",
    "Possible values:",
    "[default:",
    "[possible values:",
    "[aliases:",
    "[env:",
    "[subcommands:",
    "Print help",
    "Print help (see more with '--help')",
    "Print help (see a summary with '-h')",
    "Print version",
    "Print this message or the help of the given subcommand(s)",
    "Print help for the subcommand(s)",
];

/// clap 4.6 `error/format.rs`·`error/mod.rs` 에서 확인한 오류 문장. `{}` 가 있으면 `[[fmt]]`, 없으면 `[[text]]` 키.
#[allow(dead_code)]
pub(crate) const CLAP_ERROR_FMT: &[&str] = &[
    "error: unexpected argument '{}' found",
    "tip: to pass '{}' as a value, use '{}'",
    "tip: a similar argument exists: '{}'",
    "tip: a similar subcommand exists: '{}'",
    "tip: a similar value exists: '{}'",
    "tip: some similar subcommands exist: {}",
    "tip: some similar arguments exist: {}",
    "tip: some similar values exist: {}",
    "error: invalid value '{}' for '{}'",
    "error: invalid value '{}' for '{}': {}",
    "error: unrecognized subcommand '{}'",
    "error: the argument '{}' cannot be used multiple times",
    "error: the argument '{}' cannot be used with '{}'",
    "error: the argument '{}' cannot be used with:",
    "error: the argument '{}' cannot be used with one or more of the other specified arguments",
    "error: the subcommand '{}' cannot be used with '{}'",
    "error: the subcommand '{}' cannot be used with:",
    "error: the subcommand '{}' cannot be used with one or more of the other specified arguments",
    "error: equal sign is needed when assigning values to '{}'",
    "error: a value is required for '{}' but none was supplied",
    "error: the following required arguments were not provided:",
    "error: '{}' requires a subcommand but one was not provided",
    "error: unexpected value '{}' for '{}' found; no more were expected",
    "error: {} values required by '{}'; only {} was provided",
    "error: {} values required by '{}'; only {} were provided",
    "error: {} values required for '{}' but {} was provided",
    "error: {} values required for '{}' but {} were provided",
    "error: unexpected argument found",
    "error: unrecognized subcommand",
    "error: one of the values isn't valid for an argument",
    "error: invalid value for one of the arguments",
    "error: a subcommand is required but one was not provided",
    "error: one or more required arguments were not provided",
    "error: invalid UTF-8 was detected in one or more arguments",
    "tip: subcommand '{}' exists; to use it, remove the '--' before it",
    "error: equal is needed when assigning values to one of the arguments",
    "error: unexpected value for an argument found",
    "error: more values required for an argument",
    "error: too many or too few values for an argument",
    "error: an argument cannot be used with one or more of the other specified arguments",
    "For more information, try '{}'.",
];

// ---------------------------------------------------------------------------------------------
// 진입점
// ---------------------------------------------------------------------------------------------

/// `Cli::parse()` 대체. En 이면 upstream 경로 그대로, Ko 이면 영어 트리로 파싱하고 도움말·오류만 번역해 렌더한다.
pub(crate) fn parse_cli() -> Cli {
    if !super::is_ko() {
        return <Cli as clap::Parser>::parse();
    }
    let argv: Vec<OsString> = std::env::args_os().collect();
    let mut cmd = Cli::command();
    match cmd.try_get_matches_from_mut(argv.clone()) {
        Ok(mut m) => match Cli::from_arg_matches_mut(&mut m) {
            Ok(cli) => {
                mark_machine_mode(&cli);
                cli
            }
            Err(e) => match usable_table() {
                Some(tb) => exit_localized(e.format(&mut localized_root(tb)), tb),
                None => e.format(&mut Cli::command()).exit(),
            },
        },
        Err(e) if e.kind() == ErrorKind::DisplayVersion => e.exit(),
        Err(e) => {
            let Some(tb) = usable_table() else { e.exit() };
            DEFERRED_TABLE.with(|c| c.set(Some(tb)));
            let mut l = localized_root(tb);
            match l.try_get_matches_from_mut(argv) {
                Err(e2) if e2.kind() == e.kind() => exit_localized(e2, tb),
                _ => exit_localized(e, tb),
            }
        }
    }
}

/// 번역표가 있고 비어 있지 않을 때만 Some.
fn usable_table() -> Option<&'static Table> {
    super::table().filter(|t| !t.is_empty())
}

/// 기계용 출력(print, completions, translate, components schema, `--json-output`)이면 true.
fn is_machine_cli(cli: &Cli) -> bool {
    use crate::Commands;
    cli.verbosity.json_output
        || matches!(
            &cli.action,
            Commands::Print(_)
                | Commands::ShellCompletions(_)
                | Commands::Translate(_)
                | Commands::Components(crate::component::ComponentCommand::Schema)
        )
}

/// 기계용 출력이면 모든 번역 훅을 끈다.
fn mark_machine_mode(cli: &Cli) {
    if is_machine_cli(cli) {
        super::set_machine_mode();
    }
}

fn localized_root(table: &'static Table) -> Command {
    localized_root_with_width(table, ko_term_width())
}

/// 좁은 폭에서 `-h` 설명 칸이 0 이 되지 않도록 모든 인자의 설명을 다음 줄로 보낸다(전역 전파, 파싱 무영향).
fn localized_root_with_width(table: &'static Table, width: usize) -> Command {
    localize(Cli::command(), table)
        .next_line_help(true)
        .term_width(width)
}

/// 렌더 결과를 후처리해 색 판정에 맞게 출력하고 종료한다(종료 코드는 clap 과 같다).
fn exit_localized(e: clap::Error, table: &Table) -> ! {
    let mut out = render_error(&e, table);
    let use_stderr = e.use_stderr();
    let color = if use_stderr {
        console::colors_enabled_stderr()
    } else {
        console::colors_enabled()
    };
    if !color {
        out = console::strip_ansi_codes(&out).into_owned();
    }
    if use_stderr {
        let mut w = std::io::stderr().lock();
        let _ = w.write_all(out.as_bytes());
        let _ = w.flush();
    } else {
        let mut w = std::io::stdout().lock();
        let _ = w.write_all(out.as_bytes());
        let _ = w.flush();
    }
    std::process::exit(e.exit_code())
}

/// 종료하지 않고 렌더 + 후처리만 한다(ANSI 포함).
fn render_error(e: &clap::Error, table: &Table) -> String {
    translate_clap_output(&e.render().ansi().to_string(), e.kind(), table)
}

/// 한국어 도움말 폭: 글자당 최대 2칸이므로 clap 폭을 절반으로 둔다.
fn width_for_cols(cols: usize) -> usize {
    (cols.min(100) / 2).max(40)
}

fn ko_term_width() -> usize {
    let cols = console::Term::stdout()
        .size_checked()
        .map(|(_, c)| c as usize)
        .filter(|c| *c > 0)
        .or_else(|| {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|c| *c > 0)
        })
        .unwrap_or(100);
    width_for_cols(cols)
}

// ---------------------------------------------------------------------------------------------
// 트리 번역
// ---------------------------------------------------------------------------------------------

fn lookup(table: &'static Table, s: Option<&StyledStr>) -> Option<&'static str> {
    let s = s?.to_string();
    table.text(&s)
}

/// 명령 트리의 about/help/제목을 번역한다. 인자 id·이름·값 파서·순서는 건드리지 않는다.
pub(crate) fn localize(cmd: Command, table: &'static Table) -> Command {
    let mut cmd = cmd;
    if let Some(ko) = lookup(table, cmd.get_about()) {
        cmd = cmd.about(ko);
    }
    if let Some(ko) = lookup(table, cmd.get_long_about()) {
        cmd = cmd.long_about(ko);
    }
    if let Some(ko) = lookup(table, cmd.get_before_help()) {
        cmd = cmd.before_help(ko);
    }
    if let Some(ko) = lookup(table, cmd.get_before_long_help()) {
        cmd = cmd.before_long_help(ko);
    }
    if let Some(ko) = lookup(table, cmd.get_after_help()) {
        cmd = cmd.after_help(ko);
    }
    if let Some(ko) = lookup(table, cmd.get_after_long_help()) {
        cmd = cmd.after_long_help(ko);
    }
    if let Some(ko) = cmd
        .get_subcommand_help_heading()
        .and_then(|h| table.text(h))
    {
        cmd = cmd.subcommand_help_heading(ko);
    }
    cmd.mut_args(|mut a| {
        if let Some(ko) = lookup(table, a.get_help()) {
            a = a.help(ko);
        }
        if let Some(ko) = lookup(table, a.get_long_help()) {
            a = a.long_help(ko);
        }
        if let Some(ko) = a.get_help_heading().and_then(|h| table.text(h)) {
            a = a.help_heading(ko);
        }
        a
    })
    .mut_subcommands(|c| localize(c, table))
}

/// 지연 하위명령(`@client`/`@server`) 번역 판정(순수 함수). completions 생성 중이거나 표가 없으면 입력 그대로.
pub(crate) fn localize_deferred_with(
    cmd: Command,
    generating: bool,
    table: Option<&'static Table>,
) -> Command {
    match table {
        Some(t) if !generating => localize(cmd, t),
        _ => cmd,
    }
}

/// platform_override.rs 의 defer 클로저가 부른다. 전역 상태를 읽어 `localize_deferred_with` 에 넘긴다.
pub(crate) fn localize_deferred(cmd: Command) -> Command {
    let table = DEFERRED_TABLE.with(Cell::get);
    localize_deferred_with(
        cmd,
        GENERATING_COMPLETIONS.load(std::sync::atomic::Ordering::Relaxed),
        table,
    )
}

// ---------------------------------------------------------------------------------------------
// 렌더 후처리
// ---------------------------------------------------------------------------------------------

const LEAD_TOKENS: &[&str] = &[
    "Usage:",
    "Options:",
    "Arguments:",
    "Commands:",
    "Possible values:",
];
const INLINE_TOKENS: &[&str] = &[
    "[default:",
    "[possible values:",
    "[aliases:",
    "[env:",
    "[subcommands:",
];
/// 줄 끝 문구(긴 것 먼저).
const TAIL_PHRASES: &[&str] = &[
    "Print this message or the help of the given subcommand(s)",
    "Print help (see more with '--help')",
    "Print help (see a summary with '-h')",
    "Print help for the subcommand(s)",
    "Print version",
    "Print help",
];

/// 줄 끝의 공백과 ANSI 이스케이프를 뺀 앞부분.
fn strip_trailing(raw: &str) -> &str {
    let mut s = raw;
    loop {
        let t = s.trim_end();
        if t.len() != s.len() {
            s = t;
            continue;
        }
        if s.ends_with('m') {
            if let Some(i) = s.rfind('\x1b') {
                let mid = &s[i + 1..s.len() - 1];
                if mid.starts_with('[') && mid[1..].bytes().all(|b| b.is_ascii_digit() || b == b';')
                {
                    s = &s[..i];
                    continue;
                }
            }
        }
        return s;
    }
}

/// 줄 끝(ANSI·공백 제외)이 `phrase` 이고 그 앞이 줄 처음이나 공백이면 `ko` 로 바꾼다.
fn replace_tail(raw: &str, phrase: &str, ko: &str) -> Option<String> {
    let head = strip_trailing(raw);
    let prefix = head.strip_suffix(phrase)?;
    let plain = console::strip_ansi_codes(prefix);
    if !(plain.is_empty() || plain.ends_with(char::is_whitespace)) {
        return None;
    }
    Some(format!("{prefix}{ko}{}", &raw[head.len()..]))
}

/// `- name: 설명` 형태의 가능한 값 줄에서 설명부를 돌려준다.
fn possible_value_desc(plain: &str) -> Option<&str> {
    let rest = plain.trim_start().strip_prefix("- ")?;
    let colon = rest.find(':')?;
    let name = &rest[..colon];
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let after = &rest[colon + 1..];
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    let desc = after.trim();
    (!desc.is_empty()).then_some(desc)
}

fn translate_line<'a>(raw: &'a str, is_error: bool, table: &Table) -> Cow<'a, str> {
    if raw.trim().is_empty() {
        return Cow::Borrowed(raw);
    }
    let plain = console::strip_ansi_codes(raw);
    if is_error {
        let t = plain.trim_start();
        if t.starts_with("error: ")
            || t.starts_with("tip: ")
            || t.starts_with("For more information")
        {
            if let Cow::Owned(o) = table.tr_line(&plain) {
                return Cow::Owned(o);
            }
        }
    }
    let mut line: Cow<'a, str> = Cow::Borrowed(raw);

    for tok in LEAD_TOKENS {
        let Some(ko) = table.text(tok).filter(|k| k != tok) else {
            continue;
        };
        let ok = if *tok == "Usage:" {
            plain.trim_start().starts_with(tok)
        } else {
            plain.trim() == *tok
        };
        if !ok {
            continue;
        }
        if let Some(i) = line.find(tok) {
            line = Cow::Owned(format!("{}{}{}", &line[..i], ko, &line[i + tok.len()..]));
            break;
        }
    }

    for tok in INLINE_TOKENS {
        let Some(ko) = table.text(tok).filter(|k| k != tok) else {
            continue;
        };
        if line.contains(tok) {
            line = Cow::Owned(line.replace(tok, ko));
        }
    }

    for phrase in TAIL_PHRASES {
        let Some(ko) = table.text(phrase).filter(|k| k != phrase) else {
            continue;
        };
        if let Some(s) = replace_tail(&line, phrase, ko) {
            line = Cow::Owned(s);
            break;
        }
    }

    if let Some(desc) = possible_value_desc(&plain) {
        if let Some(ko) = table.text(desc).filter(|k| *k != desc) {
            if let Some(s) = replace_tail(&line, desc, ko) {
                line = Cow::Owned(s);
            }
        }
    }
    line
}

/// 앞 공백 칸 수.
fn indent_of(plain: &str) -> usize {
    plain.len() - plain.trim_start().len()
}

/// `[possible` / `values:` 로 갈라진 줄을 합친다. 줄 끝이 `[possible` 이고 다음 줄이 `values:` 로 시작할 때만.
fn merge_split_inline(lines: &[&str]) -> Option<String> {
    let first = console::strip_ansi_codes(lines[0]);
    if !first.trim_end().ends_with("[possible") {
        return None;
    }
    let next = console::strip_ansi_codes(lines.get(1)?);
    if !next.trim_start().starts_with("values:") {
        return None;
    }
    let head = strip_trailing(lines[0]);
    Some(format!("{head} {}", lines[1].trim_start()))
}

/// `- name: 설명` 줄의 설명이 다음 줄로 이어진 경우 이어지는 줄을 공백으로 합쳐 표를 조회한다.
/// 일치하면 번역문 한 줄과 쓴 줄 수를 돌려준다(이어지는 줄은 지운다).
fn merge_wrapped_possible_value(lines: &[&str], table: &Table) -> Option<(String, usize)> {
    let first = console::strip_ansi_codes(lines[0]);
    let desc = possible_value_desc(&first)?;
    let dash = indent_of(&first);
    let mut conts: Vec<String> = Vec::new();
    for l in &lines[1..] {
        let t = console::strip_ansi_codes(l);
        if t.trim().is_empty() || indent_of(&t) <= dash || t.trim_start().starts_with("- ") {
            break;
        }
        conts.push(t.trim().to_string());
    }
    for k in (1..=conts.len()).rev() {
        let joined = format!("{desc} {}", conts[..k].join(" "));
        let Some(ko) = table.text(&joined).filter(|ko| *ko != joined) else {
            continue;
        };
        // 첫 줄 끝의 desc 부분(앞 부분 desc 가 줄 끝)을 번역문으로 바꾼다
        if let Some(merged) = replace_tail(lines[0], desc, ko) {
            return Some((merged, k + 1));
        }
    }
    None
}

/// clap 이 렌더한 도움말·오류(ANSI 포함)의 내장 영어 문구를 줄 단위로 번역한다.
pub(crate) fn translate_clap_output(rendered_ansi: &str, kind: ErrorKind, table: &Table) -> String {
    let is_error = !matches!(
        kind,
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    let lines: Vec<&str> = rendered_ansi.split('\n').collect();
    let mut out = String::with_capacity(rendered_ansi.len());
    let mut i = 0;
    while i < lines.len() {
        if i > 0 {
            out.push('\n');
        }
        if let Some((merged, used)) = merge_wrapped(&lines[i..], table) {
            out.push_str(&translate_line(&merged, is_error, table));
            i += used;
        } else if let Some((merged, used)) = merge_wrapped_possible_value(&lines[i..], table) {
            out.push_str(&translate_line(&merged, is_error, table));
            i += used;
        } else if let Some(merged) = merge_split_inline(&lines[i..]) {
            out.push_str(&translate_line(&merged, is_error, table));
            i += 2;
        } else {
            out.push_str(&translate_line(lines[i], is_error, table));
            i += 1;
        }
    }
    out
}

/// clap 이 줄 끝 문구를 둘 이상의 줄로 나눈 경우(`Print this message or the help of` / `given subcommand(s)`)
/// 하나로 합쳐 번역한다. 나뉜 뒷줄에는 남은 단어만 있어야 한다.
fn merge_wrapped(lines: &[&str], table: &Table) -> Option<(String, usize)> {
    let first_plain = console::strip_ansi_codes(lines[0]);
    let first_plain = first_plain.trim_end();
    for phrase in TAIL_PHRASES {
        let Some(ko) = table.text(phrase).filter(|k| k != phrase) else {
            continue;
        };
        let words: Vec<&str> = phrase.split_whitespace().collect();
        for p in (1..words.len()).rev() {
            let prefix = words[..p].join(" ");
            let rest = words[p..].join(" ");
            let Some(before) = first_plain.strip_suffix(prefix.as_str()) else {
                continue;
            };
            if !(before.is_empty() || before.ends_with(char::is_whitespace)) {
                continue;
            }
            let mut acc = String::new();
            let max_lines = words.len() - p;
            for (n, l) in lines.iter().enumerate().skip(1).take(max_lines) {
                let t = console::strip_ansi_codes(l);
                let t = t.trim();
                if t.is_empty() {
                    break;
                }
                if !acc.is_empty() {
                    acc.push(' ');
                }
                acc.push_str(t);
                if acc == rest {
                    // 바로 다음 줄이 같은 들여쓰기의 이어지는 줄이면 더 긴 문구의 일부이므로 기각
                    if let Some(next) = lines.get(n + 1) {
                        let np = console::strip_ansi_codes(next);
                        if !np.trim().is_empty()
                            && indent_of(&np) == indent_of(&console::strip_ansi_codes(l))
                        {
                            break;
                        }
                    }
                    let Some(merged) = replace_tail(lines[0], &prefix, ko) else {
                        break;
                    };
                    return Some((merged, n + 1));
                }
                if !rest.starts_with(&format!("{acc} ")) {
                    break;
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// 번역 키 수집
// ---------------------------------------------------------------------------------------------

/// 번역 대상 도움말 문구 하나.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct HelpKey {
    /// 명령 경로(`dx serve --port` 등)
    pub path: String,
    /// about|long_about|help|long_help|heading|subcommand_heading|possible_value|before_help|before_long_help|after_help|after_long_help
    pub kind: &'static str,
    pub en: String,
    /// 숨김 명령·인자·값이면 true
    pub hidden: bool,
}

fn arg_label(a: &clap::Arg) -> String {
    if let Some(l) = a.get_long() {
        format!("--{l}")
    } else if let Some(s) = a.get_short() {
        format!("-{s}")
    } else {
        a.get_id().to_string()
    }
}

fn push_key(out: &mut Vec<HelpKey>, path: &str, kind: &'static str, en: String, hidden: bool) {
    if !en.is_empty() {
        out.push(HelpKey {
            path: path.to_string(),
            kind,
            en,
            hidden,
        });
    }
}

fn walk_keys(cmd: &Command, path: &str, parent_hidden: bool, out: &mut Vec<HelpKey>) {
    let hidden = parent_hidden || cmd.is_hide_set();
    let styled = |s: Option<&StyledStr>| s.map(|s| s.to_string());
    let cmd_texts: [(&'static str, Option<String>); 6] = [
        ("about", styled(cmd.get_about())),
        ("long_about", styled(cmd.get_long_about())),
        ("before_help", styled(cmd.get_before_help())),
        ("before_long_help", styled(cmd.get_before_long_help())),
        ("after_help", styled(cmd.get_after_help())),
        ("after_long_help", styled(cmd.get_after_long_help())),
    ];
    for (kind, en) in cmd_texts {
        if let Some(en) = en {
            push_key(out, path, kind, en, hidden);
        }
    }
    if let Some(h) = cmd.get_subcommand_help_heading() {
        push_key(out, path, "subcommand_heading", h.to_string(), hidden);
    }
    for a in cmd.get_arguments() {
        let apath = format!("{path} {}", arg_label(a));
        let ahidden = hidden || a.is_hide_set();
        if let Some(h) = a.get_help() {
            push_key(out, &apath, "help", h.to_string(), ahidden);
        }
        if let Some(h) = a.get_long_help() {
            push_key(out, &apath, "long_help", h.to_string(), ahidden);
        }
        if let Some(h) = a.get_help_heading() {
            push_key(out, &apath, "heading", h.to_string(), ahidden);
        }
        for pv in a.get_possible_values() {
            if let Some(h) = pv.get_help() {
                let ppath = format!("{apath} [{}]", pv.get_name());
                push_key(
                    out,
                    &ppath,
                    "possible_value",
                    h.to_string(),
                    ahidden || pv.is_hide_set(),
                );
            }
        }
    }
    for sc in cmd.get_subcommands() {
        let spath = format!("{path} {}", sc.get_name());
        walk_keys(sc, &spath, hidden, out);
    }
}

/// 명령 트리 전체(지연 하위명령 `@client`/`@server` 의 실제 타입 포함)의 번역 대상 문구.
#[allow(dead_code)]
pub(crate) fn help_keys() -> Vec<HelpKey> {
    use crate::cli::platform_override::PlatformOverrides;
    use crate::cli::serve::PlatformServeArgs;
    use clap::Subcommand;

    let mut out = Vec::new();
    let root = Cli::command();
    walk_keys(&root, "dx", false, &mut out);

    // 지연 하위명령은 선택될 때만 만들어지므로 실제 타입의 augment_subcommands 로 직접 만든다.
    let probes: [(&str, Command); 2] = [
        (
            "dx build",
            PlatformOverrides::<crate::BuildArgs>::augment_subcommands(Command::new("probe")),
        ),
        (
            "dx serve",
            PlatformOverrides::<PlatformServeArgs>::augment_subcommands(Command::new("probe")),
        ),
    ];
    for (prefix, probe) in &probes {
        for sc in probe.get_subcommands() {
            let spath = format!("{prefix} {}", sc.get_name());
            walk_keys(sc, &spath, false, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ArgMatches;
    use std::collections::{BTreeSet, HashSet};
    use std::sync::OnceLock;

    #[test]
    fn json_output_marks_machine_cli() {
        let parse = |a: &[&str]| <Cli as clap::Parser>::try_parse_from(a).unwrap();
        assert!(is_machine_cli(&parse(&["dx", "doctor", "--json-output"])));
        assert!(is_machine_cli(&parse(&["dx", "--json-output", "doctor"])));
        assert!(!is_machine_cli(&parse(&["dx", "doctor"])));
    }

    // ---- TOML 덤프 ----

    struct Entry {
        kind: &'static str,
        path: String,
        en: String,
        fmt: bool,
        hidden: bool,
    }

    fn toml_basic(s: &str) -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\t' => o.push_str("\\t"),
                c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                    o.push_str(&format!("\\u{:04X}", c as u32))
                }
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }

    /// 중복 en 은 한 번만 남긴 전체 키 목록(도움말 키 + 내장 문구 + 오류 문장).
    fn entries() -> Vec<Entry> {
        let mut seen = HashSet::new();
        let mut v: Vec<Entry> = Vec::new();
        for k in help_keys() {
            if seen.insert(k.en.clone()) {
                v.push(Entry {
                    kind: k.kind,
                    path: k.path,
                    en: k.en,
                    fmt: false,
                    hidden: k.hidden,
                });
            } else if let Some(e) = v.iter_mut().find(|e| e.en == k.en) {
                // 보이는 경로가 하나라도 있으면 숨김이 아니다
                e.hidden = e.hidden && k.hidden;
            }
        }
        for t in CLAP_BUILTIN_TEXT {
            if seen.insert(t.to_string()) {
                v.push(Entry {
                    kind: "builtin",
                    path: "clap".into(),
                    en: t.to_string(),
                    fmt: false,
                    hidden: false,
                });
            }
        }
        for t in CLAP_ERROR_FMT {
            if seen.insert(t.to_string()) {
                v.push(Entry {
                    kind: "clap_error",
                    path: "clap".into(),
                    en: t.to_string(),
                    fmt: t.contains("{}"),
                    hidden: false,
                });
            }
        }
        v
    }

    /// 실제 번역표 형식으로 출력한다. `ko_of` 가 None 이면 `ko = ""`.
    fn to_toml(entries: &[Entry], ko_of: impl Fn(usize, &Entry) -> String) -> String {
        let mut s = String::new();
        let mut last = "";
        for (i, e) in entries.iter().enumerate() {
            let section = match e.kind {
                "builtin" => "clap 내장 문구",
                "clap_error" => "clap 오류 문장",
                _ => "clap 도움말 (cli/)",
            };
            if section != last {
                s.push_str(&format!("# == 구역: {section} ==\n"));
                last = section;
            }
            s.push_str(&format!("# kind: {}\n# path: {}\n", e.kind, e.path));
            s.push_str(if e.fmt { "[[fmt]]\n" } else { "[[text]]\n" });
            s.push_str(&format!(
                "en = {}\nko = {}\n\n",
                toml_basic(&e.en),
                toml_basic(&ko_of(i, e))
            ));
        }
        s
    }

    /// 모든 문자열을 "KO:" + 원문으로 바꾸는 가짜 표. 내장 문구는 영어가 남지 않도록 "한글N" 으로 둔다.
    fn pseudo_table() -> &'static Table {
        static T: OnceLock<&'static Table> = OnceLock::new();
        T.get_or_init(|| {
            let es = entries();
            let mut src = to_toml(&es, |i, e| match e.kind {
                "builtin" => format!("한글{i}"),
                _ if e.fmt => {
                    let mut n = 0;
                    let mut out = String::from("KO:");
                    let mut rest = e.en.as_str();
                    while let Some(p) = rest.find("{}") {
                        n += 1;
                        out.push_str(&rest[..p]);
                        out.push_str(&format!("{{{n}}}"));
                        rest = &rest[p + 2..];
                    }
                    out.push_str(rest);
                    out
                }
                _ => format!("KO:{}", e.en),
            });
            // platform.rs 의 raw 오류(렌더 시 `error: ` 접두가 붙는다)
            src.push_str("[[fmt]]\nen = \"error: Unknown platform: {}\"\nko = \"KO:{1}\"\n\n");
            let (t, errs) = Table::parse(&src);
            assert!(errs.is_empty(), "pseudo table errors: {errs:#?}");
            Box::leak(Box::new(t))
        })
    }

    // ---- 렌더 도우미 ----

    fn render_help(table: &'static Table, argv: &[&str]) -> (ErrorKind, String) {
        DEFERRED_TABLE.with(|c| c.set(Some(table)));
        let mut l = localized_root_with_width(table, 50);
        let mut full = vec!["dx"];
        full.extend_from_slice(argv);
        let e = l
            .try_get_matches_from_mut(full)
            .expect_err("expected a help/error exit");
        DEFERRED_TABLE.with(|c| c.set(None));
        (e.kind(), render_error(&e, table))
    }

    /// 번역 트리를 `width` 로 렌더한 도움말(ANSI 제거).
    fn help_at(table: &'static Table, width: usize, argv: &[&str]) -> String {
        DEFERRED_TABLE.with(|c| c.set(Some(table)));
        let mut l = localized_root_with_width(table, width);
        let mut full = vec!["dx"];
        full.extend_from_slice(argv);
        let e = l.try_get_matches_from_mut(full).expect_err("help");
        DEFERRED_TABLE.with(|c| c.set(None));
        console::strip_ansi_codes(&render_error(&e, table)).into_owned()
    }

    fn english_help(width: usize, argv: &[&str]) -> String {
        let mut c = Cli::command().term_width(width);
        let mut full = vec!["dx"];
        full.extend_from_slice(argv);
        let e = c.try_get_matches_from_mut(full).expect_err("help");
        console::strip_ansi_codes(&e.render().ansi().to_string()).into_owned()
    }

    #[test]
    fn narrow_short_help_is_not_word_per_line() {
        let pseudo = pseudo_table();
        for argv in [["serve", "-h"], ["build", "-h"]] {
            let en = english_help(100, &argv);
            let en_lines = en.lines().count();
            for width in [40, 50] {
                let ko = help_at(pseudo, width, &argv);
                let ko_lines = ko.lines().count();
                // 낱말 하나만 있는 줄이 연속으로 3줄 이상 나오면 설명 칸이 0 으로 무너진 것
                let mut run = 0;
                let mut max_run = 0;
                for l in ko.lines() {
                    if l.split_whitespace().count() == 1 && l.starts_with(' ') {
                        run += 1;
                        max_run = max_run.max(run);
                    } else {
                        run = 0;
                    }
                }
                eprintln!(
                    "{argv:?} width={width}: ko_lines={ko_lines} en100_lines={en_lines} max_one_word_run={max_run}"
                );
                assert!(max_run < 3, "{argv:?} width={width}:\n{ko}");
                assert!(
                    ko_lines <= en_lines * 3,
                    "{argv:?} width={width}: {ko_lines} > 3 * {en_lines}\n{ko}"
                );
            }
        }
    }

    #[test]
    fn wrapped_phrases_merge_and_reject_partial() {
        let t: &'static Table = Box::leak(Box::new(
            Table::parse(
                "[[text]]\nen = \"Print help\"\nko = \"짧은\"\n\n[[text]]\nen = \"Print help (see more with '--help')\"\nko = \"긴 도움말\"\n\n[[text]]\nen = \"[possible values:\"\nko = \"[가능한 값:\"\n\n[[text]]\nen = \"does a long thing here\"\nko = \"번역됨\"\n",
            )
            .0,
        ));
        // 낱말 하나씩 5줄로 갈라져도 긴 문구로 합친다(짧은 문구 부분 일치 금지)
        let raw = "  -h  Print\n      help\n      (see\n      more\n      with\n      '--help')";
        let out = translate_clap_output(raw, ErrorKind::DisplayHelp, t);
        assert_eq!(out, "  -h  긴 도움말", "{out}");
        // 갈라진 `[possible values:`
        let raw = "  x  desc [possible\n     values: a, b]";
        let out = translate_clap_output(raw, ErrorKind::DisplayHelp, t);
        assert_eq!(out, "  x  desc [가능한 값: a, b]", "{out}");
        // 가능한 값 설명이 다음 줄로 이어진 경우
        let raw = "  - name: does a long\n    thing here\n  - other: x";
        let out = translate_clap_output(raw, ErrorKind::DisplayHelp, t);
        assert_eq!(out, "  - name: 번역됨\n  - other: x", "{out}");
    }

    fn fingerprint(m: &ArgMatches) -> String {
        let mut ids: Vec<String> = m.ids().map(|i| i.as_str().to_string()).collect();
        ids.sort();
        let mut s = String::new();
        for id in ids {
            let vals: Vec<std::ffi::OsString> = m
                .try_get_raw(&id)
                .ok()
                .flatten()
                .map(|v| v.map(|o| o.to_os_string()).collect())
                .unwrap_or_default();
            s.push_str(&format!("{id}|{:?}|{vals:?};", m.value_source(&id)));
        }
        if let Some((name, sub)) = m.subcommand() {
            s.push_str(&format!("=>{name}({})", fingerprint(sub)));
        }
        s
    }

    type Outcome = Result<String, ErrorKind>;

    fn outcome(mut cmd: Command, argv: &[&str]) -> (Outcome, Option<bool>) {
        match cmd.try_get_matches_from_mut(argv) {
            Ok(mut m) => {
                let fp = fingerprint(&m);
                let parsed = Cli::from_arg_matches_mut(&mut m).is_ok();
                (Ok(fp), Some(parsed))
            }
            Err(e) => (Err(e.kind()), None),
        }
    }

    const ARGV: &[&[&str]] = &[
        &["dx", "serve"],
        &[
            "dx",
            "serve",
            "--port",
            "9",
            "--release",
            "--platform",
            "web",
        ],
        &[
            "dx",
            "build",
            "--release",
            "@client",
            "--platform",
            "web",
            "@server",
            "--features",
            "a",
        ],
        &["dx", "build", "@server", "--release", "@client"],
        &["dx", "build", "@client", "@server", "@client", "--release"],
        &[
            "dx",
            "bundle",
            "--platform",
            "desktop",
            "--package-types",
            "msi",
        ],
        &[
            "dx",
            "bundle",
            "--platform",
            "desktop",
            "@client",
            "--release",
            "@server",
            "--platform",
            "server",
        ],
        &["dx", "--verbose", "build"],
        &["dx", "build", "--json-output"],
        &["dx", "run", "--web"],
        &["dx", "check"],
        &["dx", "fmt", "--check"],
        &["dx", "config", "set", "always-on-top", "true"],
        &["dx", "components", "list"],
        &["dx", "print", "client-args"],
        &["dx", "serve", "@client", "--help"],
        &["dx", "serve", "@server", "-h"],
        &["dx", "build", "@client", "--help"],
        &["dx", "--help"],
        &["dx", "help", "serve"],
        &["dx"],
        &["dx", "bogus"],
        &["dx", "servee"],
        &["dx", "serve", "--bogus"],
        &["dx", "serve", "--port", "x"],
        &["dx", "build", "--platform", "zz"],
        &["dx", "build", "--web", "--desktop"],
        &["dx", "build", "--release", "--release"],
    ];

    #[test]
    fn localized_tree_parses_like_english() {
        let pseudo = pseudo_table();
        for argv in ARGV {
            DEFERRED_TABLE.with(|c| c.set(None));
            let en = outcome(Cli::command(), argv);
            DEFERRED_TABLE.with(|c| c.set(Some(pseudo)));
            let ko = outcome(localize(Cli::command(), pseudo), argv);
            DEFERRED_TABLE.with(|c| c.set(None));
            assert_eq!(en, ko, "argv = {argv:?}");
        }
        // 지연 하위명령 안의 인자까지 번역됐는지(F4)
        let (kind, out) = render_help(pseudo, &["build", "@client", "--help"]);
        assert_eq!(kind, ErrorKind::DisplayHelp);
        assert!(
            out.contains("KO:"),
            "no translation in deferred help:\n{out}"
        );
        let (_, out) = render_help(pseudo, &["serve", "@server", "-h"]);
        assert!(out.contains("KO:"), "{out}");
        // 도움말 렌더 결과에는 원본 영어 about 이 남지 않는다
        let (_, out) = render_help(pseudo, &["build", "--help"]);
        assert!(
            out.contains("KO:Specify the arguments for the client"),
            "{out}"
        );
    }

    #[test]
    fn help_keys_cover_tree() {
        let keys = help_keys();
        assert!(keys.len() > 200, "only {} keys", keys.len());
        assert!(keys.iter().any(|k| k.kind == "possible_value"));
        assert!(
            keys.iter()
                .any(|k| k.en.contains("Targeting webview renderer"))
        );
        assert!(keys.iter().any(|k| k.kind == "heading"));
        assert!(keys.iter().any(|k| k.path.starts_with("dx build @client")));
        assert!(keys.iter().any(|k| k.path.starts_with("dx serve @server")));
        assert!(
            keys.iter()
                .any(|k| k.en == "Specify the arguments for the client build")
        );
        assert!(keys.iter().any(|k| k.hidden));
        let kinds: BTreeSet<_> = keys.iter().map(|k| k.kind).collect();
        eprintln!("kinds = {kinds:?}");
    }

    fn all_paths(cmd: &Command, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        out.push(prefix.clone());
        for sc in cmd.get_subcommands() {
            prefix.push(sc.get_name().to_string());
            all_paths(sc, prefix, out);
            prefix.pop();
        }
    }

    #[test]
    fn localized_help_has_no_builtin_english() {
        let pseudo = pseudo_table();
        let mut paths = Vec::new();
        all_paths(&Cli::command(), &mut Vec::new(), &mut paths);
        for extra in [
            ["build", "@client"],
            ["serve", "@server"],
            ["bundle", "@client"],
        ] {
            paths.push(extra.iter().map(|s| s.to_string()).collect());
        }
        for p in &paths {
            for flag in ["--help", "-h"] {
                let mut argv: Vec<&str> = p.iter().map(String::as_str).collect();
                argv.push(flag);
                let (kind, out) = render_help(pseudo, &argv);
                assert_eq!(kind, ErrorKind::DisplayHelp, "{argv:?}");
                let plain = console::strip_ansi_codes(&out).into_owned();
                for l in plain.lines() {
                    let t = l.trim();
                    assert!(!l.starts_with("Usage:"), "{argv:?}: {l}");
                    assert!(
                        !matches!(
                            t,
                            "Options:" | "Arguments:" | "Commands:" | "Possible values:"
                        ),
                        "{argv:?}: {l}"
                    );
                    for bad in ["Print help", "Print version", "Print this message"] {
                        assert!(!l.contains(bad), "{argv:?}: {l}");
                    }
                }
            }
        }
    }

    #[test]
    fn completions_are_not_localized() {
        use clap::Subcommand;
        let pseudo = pseudo_table();
        let probe = crate::cli::platform_override::PlatformOverrides::<crate::BuildArgs>::augment_subcommands(
            Command::new("probe"),
        );
        let about = |c: &Command| {
            c.get_subcommands()
                .find(|s| s.get_name() == "@client")
                .and_then(|s| s.get_about().map(|a| a.to_string()))
        };
        let en = about(&probe);
        assert_eq!(
            en.as_deref(),
            Some("Specify the arguments for the client build")
        );
        // 생성 중이면 표가 있어도 입력 그대로
        let kept = localize_deferred_with(probe.clone(), true, Some(pseudo));
        assert_eq!(about(&kept), en);
        // 표가 없어도 그대로
        assert_eq!(
            about(&localize_deferred_with(probe.clone(), false, None)),
            en
        );
        // 생성 중이 아니고 표가 있으면 번역
        let ko = localize_deferred_with(probe, false, Some(pseudo));
        assert_eq!(
            about(&ko).as_deref(),
            Some("KO:Specify the arguments for the client build")
        );
    }

    #[test]
    fn ko_term_width_bounds() {
        assert_eq!(width_for_cols(0), 40);
        assert_eq!(width_for_cols(60), 40);
        assert_eq!(width_for_cols(80), 40);
        assert_eq!(width_for_cols(100), 50);
        assert_eq!(width_for_cols(200), 50);
        assert_eq!(width_for_cols(10_000), 50);
        let w = ko_term_width();
        assert!((40..=50).contains(&w), "{w}");
    }

    #[test]
    fn from_arg_matches_error_path_is_translated() {
        let pseudo = pseudo_table();
        let e = clap::Error::raw(ErrorKind::ValueValidation, "Unknown platform: zz");
        let out = render_error(&e, pseudo);
        assert!(out.contains("KO:zz"), "{out}");
        assert!(!out.contains("Unknown platform"), "{out}");
        // 표가 비어 있으면 영어 그대로
        let same = render_error(&e, &Table::default());
        assert!(same.contains("error: Unknown platform: zz"), "{same}");
    }

    #[test]
    fn parse_error_lines_are_translated() {
        let pseudo = pseudo_table();
        let (kind, out) = render_help(pseudo, &["serve", "--bogus"]);
        assert_eq!(kind, ErrorKind::UnknownArgument);
        let plain = console::strip_ansi_codes(&out).into_owned();
        assert!(
            plain.contains("KO:error: unexpected argument '--bogus' found"),
            "{plain}"
        );
        assert!(
            plain.contains("KO:For more information, try '--help'."),
            "{plain}"
        );
        assert!(!plain.contains("\nUsage:"), "{plain}");
    }

    #[test]
    fn builtin_tokens_keep_ansi_and_skip_untranslated() {
        let t: &'static Table = Box::leak(Box::new(
            Table::parse(
                "[[text]]\nen = \"Usage:\"\nko = \"사용법:\"\n\n[[text]]\nen = \"[default:\"\nko = \"[기본값:\"\n\n[[text]]\nen = \"Print help\"\nko = \"도움말 출력\"\n",
            )
            .0,
        ));
        let raw = "\x1b[1m\x1b[32mUsage:\x1b[0m dx serve\n  -h, --help  Print help\x1b[0m\n  --x  desc [default: 1]\nOptions:\nplain Print helpful";
        let out = translate_clap_output(raw, ErrorKind::DisplayHelp, t);
        assert_eq!(
            out,
            "\x1b[1m\x1b[32m사용법:\x1b[0m dx serve\n  -h, --help  도움말 출력\x1b[0m\n  --x  desc [기본값: 1]\nOptions:\nplain Print helpful"
        );
    }

    // ---- 키 덤프·번역 검증 ----

    #[test]
    #[ignore]
    fn dump_help_keys() {
        let path = std::env::var("DX_I18N_DUMP").expect("set DX_I18N_DUMP to the output path");
        let es = entries();
        let body = to_toml(&es, |_, _| String::new());
        let header = "# clap 도움말 키 덤프 (dump_help_keys 가 생성, ko = \"\" 는 미번역)\n\n";
        std::fs::write(&path, format!("{header}{body}")).unwrap();
        let mut by_kind = std::collections::BTreeMap::new();
        for e in &es {
            *by_kind.entry(e.kind).or_insert(0usize) += 1;
        }
        eprintln!("dumped {} keys to {path}: {by_kind:?}", es.len());
        let _ = es.iter().filter(|e| e.hidden).count();
    }

    #[derive(serde::Deserialize)]
    struct RawEntry {
        en: String,
        ko: String,
    }

    #[derive(serde::Deserialize)]
    struct RawTable {
        #[serde(default)]
        text: Vec<RawEntry>,
        #[serde(default)]
        fmt: Vec<RawEntry>,
    }

    #[test]
    fn help_keys_fully_translated() {
        // 전역 KO_TABLE/table_for(Ko) 는 부르지 않는다(LOADS 테스트 보호).
        let src = crate::i18n::ko_source();
        if Table::parse(src).0.is_empty() {
            eprintln!("skip: 번역표 비어 있음");
            return;
        }
        let raw: RawTable = toml::from_str(src).expect("ko.toml parses");
        let texts: HashSet<&str> = raw
            .text
            .iter()
            .filter(|e| !e.ko.is_empty())
            .map(|e| e.en.as_str())
            .collect();
        let fmts: HashSet<&str> = raw
            .fmt
            .iter()
            .filter(|e| !e.ko.is_empty())
            .map(|e| e.en.trim())
            .collect();
        let mut missing = Vec::new();
        let mut checked = 0;
        for e in entries() {
            if e.hidden {
                continue;
            }
            checked += 1;
            let ok = if e.fmt {
                fmts.contains(e.en.as_str())
            } else {
                texts.contains(e.en.as_str())
            };
            if !ok {
                missing.push(format!("[{}] {} :: {}", e.kind, e.path, e.en));
            }
        }
        println!("checked {checked} keys");
        assert!(
            missing.is_empty(),
            "{} untranslated:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }
}
