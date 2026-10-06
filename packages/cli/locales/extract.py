#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dx CLI 번역표(ko.toml) 점검 도구 (Python 3.10 표준 라이브러리만).

소스(packages/cli/src/**/*.rs)에서 사람이 보는 영어 메시지를 뽑아 번역표와 비교한다.
명령: scan / skeleton / merge / check / check-part / selftest   (자세한 사용법: `extract.py --help`)
규칙의 정본은 docs/task-id/ko-i18n/plan.md §2.3·§5.1·§15 이고, 포크 안내서는 locales/README.md.

도움말 키 덤프 형식(`--help-keys`; Rust 테스트 i18n::clap::tests::dump_help_keys 가 만든다):
번역표와 같은 스키마([[text]]/[[fmt]], ko = "")에 항목마다 주석 두 줄을 단다.
    # kind: <about|long_about|help|long_help|heading|subcommand_heading|possible_value|
    #        before_help|after_help|builtin|clap_error>
    # path: <명령 경로, 예: dx build>
    [[text]]
    en = "Build the Dioxus project and all of its assets"
    ko = ""

재현율 게이트(scan --unclassified): 비테스트 코드의 문자열 리터럴 중 "알파벳 4자 이상 + 공백 포함"
(자리표시자 {...} 는 알파벳 수에서 뺀다)인 것이 (1) 추출 규칙에 잡히거나 (2) 제외 범주(debug!·panic!·
assert!·expect·clap 속성·패턴 비교·프로세스 인자 등)이거나 (3) extract_allow.txt 에 있어야 한다.
어느 쪽도 아니면 파일:줄과 함께 출력하고 종료코드 1 이다.
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
LOCALES = HERE
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))  # packages/cli/locales -> 저장소 루트
SRC = os.path.join(ROOT, "packages", "cli", "src")
KO_TOML = os.path.join(LOCALES, "ko.toml")
ALLOW_FILE = os.path.join(LOCALES, "extract_allow.txt")

MIN_LITERAL_ALPHA = 4
# 열린 끝(스타일 전용 묶음을 뺀 첫·끝 조각이 일반 값 묶음) 서식 문자열의 리터럴 알파벳 최소 개수
MIN_OPEN_LITERAL_LEN = 8   # 열린 끝 서식의 리터럴 전체 글자 수(공백·문장부호 포함) 최소
ZONES = ["T1", "T2", "T3", "T4", "T5"]
ZONE_TITLE = {
    "T1": "도움말·clap (런타임 덤프)",
    "T2": "루트 파일과 cli/ (doctor 제외)",
    "T3": "build/",
    "T4": "bundler/ opt/ check/ config/ 등 나머지",
    "T5": "serve/ + doctor 블록 + TUI 고정 문구",
}
DUMP_KINDS = {
    "about", "long_about", "help", "long_help", "heading", "subcommand_heading",
    "possible_value", "before_help", "after_help", "builtin", "clap_error",
}


# ---------------------------------------------------------------------------
# 1. Rust 렉서
# ---------------------------------------------------------------------------

class Tok:
    __slots__ = ("k", "v", "line", "pos", "lines", "raw")

    def __init__(self, k, v, line, pos, lines=None, raw=False):
        self.k = k          # id str bstr num char life p
        self.v = v          # 식별자/구두점/디코드된 문자열
        self.line = line    # 시작 줄(1부터)
        self.pos = pos      # 소스 안 문자 위치
        self.lines = lines  # str: 디코드된 각 줄의 소스 줄 번호 목록
        self.raw = raw

    def __repr__(self):
        return "Tok(%s,%r,%d)" % (self.k, self.v, self.line)


class LexError(Exception):
    pass


_ID_START = re.compile(r"[A-Za-z_\u0080-￿]")
_ID_CONT = re.compile(r"[A-Za-z0-9_\u0080-￿]")


def _decode_escape(src, i, line, out, out_lines):
    """src[i] == '\\' 위치의 이스케이프를 해석한다. (새 i, 새 line) 반환."""
    c = src[i + 1] if i + 1 < len(src) else ""
    if c == "\n" or (c == "\r" and src[i + 2:i + 3] == "\n"):
        # 줄 이음: 개행과 다음 줄 선행 공백 제거
        j = i + 1
        while j < len(src) and src[j] in " \t\r\n":
            if src[j] == "\n":
                line += 1
            j += 1
        if out and out[-1] == "\n" and out_lines:
            out_lines[-1] = line   # 줄 이음 뒤 조각은 이음 뒤 줄에 있다
        return j, line
    simple = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", "0": "\0", "'": "'", '"': '"'}
    if c in simple:
        ch = simple[c]
        out.append(ch)
        if ch == "\n":
            out_lines.append(line)
        return i + 2, line
    if c == "x":
        out.append(chr(int(src[i + 2:i + 4], 16)))
        return i + 4, line
    if c == "u":
        j = src.index("}", i)
        out.append(chr(int(src[i + 3:j].replace("_", ""), 16)))
        return j + 1, line
    raise LexError("알 수 없는 이스케이프 \\%s (줄 %d)" % (c, line))


def lex_string(src, i, line):
    """src[i] == '"' 에서 시작하는 일반 문자열. (값, 줄목록, 새 i, 새 line)"""
    assert src[i] == '"'
    i += 1
    out = []
    out_lines = [line]
    while True:
        if i >= len(src):
            raise LexError("닫히지 않은 문자열 (줄 %d)" % line)
        c = src[i]
        if c == '"':
            return "".join(out), out_lines, i + 1, line
        if c == "\\":
            i, line = _decode_escape(src, i, line, out, out_lines)
            continue
        if c == "\r" and src[i + 1:i + 2] == "\n":
            i += 1
            continue
        out.append(c)
        i += 1
        if c == "\n":
            line += 1
            out_lines.append(line)


def lex_raw_string(src, i, line):
    """src[i] == 'r' 이고 뒤에 #* 와 '"' 가 오는 raw 문자열. 아니면 None."""
    j = i + 1
    n = 0
    while j < len(src) and src[j] == "#":
        n += 1
        j += 1
    if j >= len(src) or src[j] != '"':
        return None
    j += 1
    end = '"' + "#" * n
    k = src.find(end, j)
    if k < 0:
        raise LexError("닫히지 않은 raw 문자열 (줄 %d)" % line)
    body = src[j:k].replace("\r\n", "\n")
    out_lines = [line]
    for ch in body:
        if ch == "\n":
            line += 1
            out_lines.append(line)
    return body, out_lines, k + len(end), line


def lex(src):
    """Rust 소스를 토큰열로. 주석(중첩 블록 주석 포함)은 버린다."""
    toks = []
    i = 0
    n = len(src)
    line = 1
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if c in " \t\r":
            i += 1
            continue
        if c == "/" and src[i + 1:i + 2] == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and src[i + 1:i + 2] == "*":
            depth = 1
            i += 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    if src[i] == "\n":
                        line += 1
                    i += 1
            continue
        # 문자열 계열 접두 (b"..", br"..", r"..", r#".."#, c"..")
        if c in "brc" and i + 1 < n:
            pre = c
            j = i + 1
            if c in "bc" and src[j:j + 1] == "r":
                pre += "r"
                j += 1
            if pre in ("r", "br", "cr"):
                res = lex_raw_string(src, j - 1, line)
                if res is not None:
                    body, ol, ni, nl = res
                    kind = "str" if pre == "r" or pre == "cr" else "bstr"
                    toks.append(Tok(kind, body, line, i, ol, True))
                    i, line = ni, nl
                    continue
            elif pre in ("b", "c") and src[j:j + 1] == '"':
                val, ol, ni, nl = lex_string(src, j, line)
                kind = "bstr" if pre == "b" else "str"
                toks.append(Tok(kind, val, line, i, ol))
                i, line = ni, nl
                continue
            elif pre == "b" and src[j:j + 1] == "'":
                k = j + 1
                if src[k] == "\\":
                    k += 2
                    while src[k] != "'":
                        k += 1
                else:
                    k += 1
                toks.append(Tok("char", src[j:k + 1], line, i))
                i = k + 1
                continue
        if c == '"':
            val, ol, ni, nl = lex_string(src, i, line)
            toks.append(Tok("str", val, line, i, ol))
            i, line = ni, nl
            continue
        if c == "'":
            # 문자 리터럴 vs 수명
            if src[i + 1:i + 2] == "\\":
                esc = src[i + 2:i + 3]
                if esc == "u":
                    q = src.index("}", i + 2) + 1
                elif esc == "x":
                    q = i + 5
                else:
                    q = i + 3
                toks.append(Tok("char", src[i:q + 1], line, i))
                i = q + 1
                continue
            if src[i + 2:i + 3] == "'":
                toks.append(Tok("char", src[i:i + 3], line, i))
                i += 3
                continue
            j = i + 1
            while j < n and _ID_CONT.match(src[j]):
                j += 1
            toks.append(Tok("life", src[i:j], line, i))
            i = j
            continue
        if c == "r" and src[i + 1:i + 2] == "#" and i + 2 < n and _ID_START.match(src[i + 2]):
            j = i + 2
            while j < n and _ID_CONT.match(src[j]):
                j += 1
            toks.append(Tok("id", src[i + 2:j], line, i))
            i = j
            continue
        if _ID_START.match(c):
            j = i + 1
            while j < n and _ID_CONT.match(src[j]):
                j += 1
            toks.append(Tok("id", src[i:j], line, i))
            i = j
            continue
        if c.isdigit():
            j = i + 1
            while j < n and (src[j].isalnum() or src[j] == "_" or (
                    src[j] == "." and src[j + 1:j + 2].isdigit())):
                j += 1
            toks.append(Tok("num", src[i:j], line, i))
            i = j
            continue
        toks.append(Tok("p", c, line, i))
        i += 1
    return toks


OPEN = {"(": ")", "[": "]", "{": "}"}
CLOSE = {")", "]", "}"}


def match_table(toks):
    """여는 괄호 인덱스 -> 닫는 괄호 인덱스 (그리고 반대)."""
    m = {}
    stack = []
    for idx, t in enumerate(toks):
        if t.k != "p":
            continue
        if t.v in OPEN:
            stack.append(idx)
        elif t.v in CLOSE:
            if stack:
                o = stack.pop()
                m[o] = idx
                m[idx] = o
    return m


def is_p(t, v):
    return t.k == "p" and t.v == v


def is_id(t, v=None):
    return t.k == "id" and (v is None or t.v == v)


def skip_group(toks, m, i):
    """toks[i] 가 여는 괄호이면 짝 다음 인덱스, 아니면 i+1."""
    if toks[i].k == "p" and toks[i].v in OPEN and i in m:
        return m[i] + 1
    return i + 1


# ---------------------------------------------------------------------------
# 2. 서식 문자열 해석 (Rust format! 문법의 자리표시자)
# ---------------------------------------------------------------------------

class FmtError(Exception):
    pass


class Placeholder:
    __slots__ = ("start", "end", "raw", "arg")

    def __init__(self, start, end, raw, arg):
        self.start = start  # 문자열 안 위치(여는 중괄호)
        self.end = end      # 닫는 중괄호 다음 위치
        self.raw = raw      # "{GLOW_STYLE:#}" 처럼 원문
        self.arg = arg      # 인자 이름("" = 다음 위치 인자, "0" = 번호, 식별자)


def parse_placeholders(s):
    """서식 문자열의 자리표시자 목록. `{{` `}}` 는 건너뛴다. 짝 없는 `}` 이면 FmtError."""
    out = []
    i = 0
    n = len(s)
    while i < n:
        c = s[i]
        if c == "{":
            if s[i + 1:i + 2] == "{":
                i += 2
                continue
            j = s.find("}", i)
            if j < 0:
                raise FmtError("닫히지 않은 중괄호")
            inner = s[i + 1:j]
            arg = inner.split(":", 1)[0].strip()
            out.append(Placeholder(i, j + 1, s[i:j + 1], arg))
            i = j + 1
        elif c == "}":
            if s[i + 1:i + 2] == "}":
                i += 2
                continue
            raise FmtError("짝 없는 }")
        else:
            i += 1
    return out


def letters(s):
    return sum(1 for ch in s if ch.isalpha())


def literal_alpha_of(s, phs):
    """자리표시자를 뺀 리터럴의 알파벳 수."""
    keep = []
    last = 0
    for p in phs:
        keep.append(s[last:p.start])
        last = p.end
    keep.append(s[last:])
    return letters("".join(keep))


def literal_len_of(s, phs):
    """자리표시자를 뺀 리터럴의 전체 글자 수(공백·문장부호 포함)."""
    keep = []
    last = 0
    for p in phs:
        keep.append(s[last:p.start])
        last = p.end
    keep.append(s[last:])
    return len("".join(keep).replace("{{", "{").replace("}}", "}"))


def group_placeholders(s, phs):
    """리터럴 없이 붙은 자리표시자를 한 묶음으로. 묶음 = [Placeholder...] 목록."""
    groups = []
    prev_end = None
    for p in phs:
        if prev_end is not None and p.start == prev_end:
            groups[-1].append(p)
        else:
            groups.append([p])
        prev_end = p.end
    return groups


def open_end_of(text, spans):
    """열린 끝인가. spans = [(시작, 끝, 인자 이름)] (text 안의 자리표시자 위치).
    인접한 자리표시자는 한 묶음이고, 모든 인자 이름이 `_STYLE` 로 끝나는 묶음은 스타일 전용이라 뺀다.
    남은 조각의 첫 조각이나 끝 조각이 값 묶음이면 열린 끝이다."""
    pieces = []   # "L" 리터럴, "G" 일반 묶음, "S" 스타일 전용 묶음
    last = 0
    prev_end = None
    for (a, b, arg) in spans:
        if a > last:
            pieces.append("L")
        sty = arg.endswith("_STYLE")
        if prev_end is not None and a == prev_end and pieces and pieces[-1] in "GS":
            if not sty:
                pieces[-1] = "G"
        else:
            pieces.append("S" if sty else "G")
        last = prev_end = b
    if last < len(text):
        pieces.append("L")
    pieces = [x for x in pieces if x != "S"]
    return bool(pieces) and (pieces[0] == "G" or pieces[-1] == "G")


def fmt_is_open(en):
    return open_end_of(en, [(p.start, p.end, p.arg) for p in parse_placeholders(en)])


def fmt_groups_count(en):
    """[[fmt]] en 의 값 묶음 수 (파싱 실패 시 FmtError)."""
    return len(group_placeholders(en, parse_placeholders(en)))


def fmt_literal_alpha(en):
    return literal_alpha_of(en, parse_placeholders(en))


def fmt_edges(en):
    """(처음이 묶음인가, 끝이 묶음인가)"""
    phs = parse_placeholders(en)
    if not phs:
        return False, False
    return phs[0].start == 0, phs[-1].end == len(en)


def parse_ko_template(ko, groups):
    """ko 의 {N} 참조 목록과 오류. ({{ }} 이스케이프). (refs, errors)"""
    refs = []
    errs = []
    i = 0
    n = len(ko)
    while i < n:
        c = ko[i]
        if c == "{":
            if ko[i + 1:i + 2] == "{":
                i += 2
                continue
            j = ko.find("}", i)
            if j < 0:
                errs.append("닫히지 않은 {")
                break
            body = ko[i + 1:j]
            if not body.isdigit():
                errs.append("잘못된 참조 {%s}" % body)
            else:
                k = int(body)
                if k < 1 or k > groups:
                    errs.append("범위 밖 참조 {%d} (묶음 %d개)" % (k, groups))
                refs.append((i, j + 1, k))
            i = j + 1
        elif c == "}":
            if ko[i + 1:i + 2] == "}":
                i += 2
                continue
            errs.append("짝 없는 }")
            i += 1
        else:
            i += 1
    return refs, errs


# ---------------------------------------------------------------------------
# 3. 소스 분석: 후보 추출
# ---------------------------------------------------------------------------

# 서식 문자열을 받는 매크로 (첫 문자열 인자 = 서식)
LOG_MACROS = {"info", "warn", "error"}
BAIL_MACROS = {"bail", "anyhow"}
PRINT_MACROS = {"println", "eprintln", "print", "eprint"}
WRITE_MACROS = {"write", "writeln"}
FORMAT_MACROS = {"format", "write", "writeln", "println", "eprintln", "print", "eprint"}
# 사람용 println!/eprintln! 허용 파일(§4.4 "사람" 행) — 그 밖의 파일 출력은 기계용
PRINT_HUMAN_FILES = {
    "workspace.rs", "cli/create.rs", "cli/doctor.rs", "cli/component.rs",
    "cli/autoformat.rs", "cli/bundle.rs",
}
# write!/writeln!(f, "…") 와 format!("…") 를 사람용 메시지로 보는 파일
WRITE_HUMAN_FILES = {"check/issues.rs", "platform.rs"}
FORMAT_HUMAN_FILES = {"check/issues.rs"}

# 재현율 게이트의 '제외 범주' (사람 화면 메시지가 아닌 문맥). 범주 이름은 보고용.
EXCLUDED_MACROS = {
    "debug": "debug 전용", "trace": "debug 전용",
    "panic": "패닉·단언", "unreachable": "패닉·단언", "unimplemented": "패닉·단언",
    "todo": "패닉·단언", "assert": "패닉·단언", "assert_eq": "패닉·단언",
    "assert_ne": "패닉·단언", "debug_assert": "패닉·단언",
    "debug_assert_eq": "패닉·단언", "debug_assert_ne": "패닉·단언",
    "json": "기계 데이터", "schema_for": "기계 데이터", "deserialize_const": "기계 데이터",
    "include_str": "기계 데이터", "include_bytes": "기계 데이터", "include": "기계 데이터",
    "include_dir": "기계 데이터", "env": "기계 데이터", "option_env": "기계 데이터",
    "concat": "기계 데이터", "stringify": "기계 데이터", "compile_error": "기계 데이터",
    "cfg": "조건·매칭", "matches": "조건·매칭",
    "pin_mut": "조건·매칭",
    "rsx": "에셋·rsx", "asset": "에셋·rsx", "arg": "clap 매크로(덤프)",
}
EXCLUDED_METHODS = {}
for _n in ("contains", "starts_with", "ends_with", "find", "rfind", "split", "rsplit",
           "split_once", "rsplit_once", "splitn", "replace", "replacen", "strip_prefix",
           "strip_suffix", "trim_start_matches", "trim_end_matches", "trim_matches",
           "matches", "eq", "ne", "contains_key"):
    EXCLUDED_METHODS[_n] = "패턴·비교"
for _n in ("arg", "args", "env", "envs", "env_remove", "var", "var_os", "current_dir"):
    EXCLUDED_METHODS[_n] = "프로세스·환경"
for _n in ("help", "long_help", "about", "long_about", "help_heading",
           "subcommand_help_heading", "before_help", "after_help", "before_long_help",
           "after_long_help", "value_name", "long", "short", "visible_alias", "alias",
           "next_help_heading", "override_usage"):
    EXCLUDED_METHODS[_n] = "clap 빌더(덤프)"
for _n in ("expect", "expect_err"):
    EXCLUDED_METHODS[_n] = "패닉·단언"
# 이 이름의 함수·메서드 호출 안 문자열은 내부 식별자
EXCLUDED_CALLS = {"profile_phase": "내부 식별자"}
CLOSURE_METHODS = {"context", "with_context", "map_err", "ok_or_else"}

STATUS_FNS = {"status_compiling_native_plugins"}
T_FNS = {"t", "t_pad"}
TR_FNS = {"tr_line", "tr_text", "tr_error_report"}
# 구조체 리터럴 필드 중 사람에게 보이는 문구를 담는 것
# 문구를 담는 const/static 이름(그 밖의 const 는 URL·템플릿·해시 등 기계용)
CONST_MSG_NAME = re.compile(r"ERR|MSG|MESSAGE|WARN|HINT")
# subject 는 뺀다: 값이 식별자(`Cargo.toml [target.*]` 등)뿐이고 끼워 넣는 warn! 쪽에 번역 경로가 없다
HUMAN_FIELDS = {"build_message", "detail", "note"}
# 문자열을 값으로 들고 있다가 나중에 화면에 나가는 자리(파일, 앞부분) — 호출 위치로 잡을 수 없는 사례
EXTRA_SITES = [
    ("build/link.rs", "linker exited with status"),
    ("serve/runner.rs", "(note: source files in newly-added workspace members"),
    ("serve/server.rs", "Unhandled internal error"),
    ("serve/mod.rs", "Learn more at"),
]
# 파일(·함수) 안 문자열을 전부 화면 문구로 보는 모드: (파일, 함수이름 정규식 또는 None, 최소 알파벳, 공백 필요)
ALL_MODES = [
    ("cli/doctor.rs", None, 4, True),
    ("check/issues.rs", None, 4, True),
    ("serve/output.rs", r"^render", 3, False),
    ("serve/runner.rs", r"^hotreload_mode_label$", 4, False),
]
# ALL 모드에서 건너뛰는 문자열(URL·기호·식별자)
ALL_IGNORE = re.compile(r"://|^[^A-Za-z]*$|^https?:|^\s*\{[^{}]*\}\s*:?\s*$")


def zone_of(rel):
    if rel == "cli/doctor.rs" or rel.startswith("serve/"):
        return "T5"
    if rel.startswith("build/"):
        return "T3"
    if rel.startswith("cli/") or "/" not in rel:
        return "T2"
    return "T4"


class Cand:
    __slots__ = ("file", "line", "kind", "en", "rule", "zone", "vals", "tok")

    def __init__(self, file, line, kind, en, rule, vals="", tok=-1):
        self.file = file
        self.line = line
        self.kind = kind
        self.en = en
        self.rule = rule
        self.zone = zone_of(file)
        self.vals = vals
        self.tok = tok

    def as_dict(self):
        return {
            "file": self.file, "line": self.line, "kind": self.kind, "key": self.en,
            "rule": self.rule, "zone": self.zone,
        }


class Analysis:
    """파일 하나의 토큰 분석."""

    def __init__(self, rel, src):
        self.rel = rel
        self.src = src
        self.errors = []    # (파일, 줄, 메시지) — 소스 규약 위반
        self.toks = lex(src)
        self.m = match_table(self.toks)
        n = len(self.toks)
        self.parent = [-1] * n
        stack = []
        for i, t in enumerate(self.toks):
            self.parent[i] = stack[-1] if stack else -1
            if t.k == "p":
                if t.v in OPEN:
                    stack.append(i)
                elif t.v in CLOSE and stack:
                    stack.pop()
        self.excluded = self._test_excluded()

    # -- #[cfg(test)] / #[test] 항목 제외 ----------------------------------
    def _attr_is_test(self, lo, hi):
        """attr 내부 토큰 [lo,hi) 가 테스트용인지."""
        ids = [t.v for t in self.toks[lo:hi] if t.k == "id"]
        if not ids:
            return False
        if ids[0] == "test" and len(ids) == 1:
            return True
        if ids[-1] == "test" and ids[0] in ("tokio", "rstest", "async_std", "wasm_bindgen_test"):
            return True
        if ids[0] == "cfg" and "test" in ids and "not" not in ids:
            return True
        return False

    def _test_excluded(self):
        toks = self.toks
        n = len(toks)
        ex = [False] * n
        i = 0
        while i < n:
            t = toks[i]
            if is_p(t, "#"):
                j = i + 1
                inner = False
                if j < n and is_p(toks[j], "!"):
                    inner = True
                    j += 1
                if j < n and is_p(toks[j], "[") and j in self.m:
                    end = self.m[j]
                    if self._attr_is_test(j + 1, end):
                        if inner:
                            for k in range(n):
                                ex[k] = True
                            return ex
                        # 뒤따르는 항목 하나를 통째로 제외
                        k = end + 1
                        while k < n:
                            tk = toks[k]
                            if is_p(tk, ";"):
                                k += 1
                                break
                            if is_p(tk, "{"):
                                k = self.m[k] + 1 if k in self.m else n
                                break
                            if tk.k == "p" and tk.v in "([" and k in self.m:
                                k = self.m[k] + 1
                                continue
                            k += 1
                        for q in range(i, min(k, n)):
                            ex[q] = True
                        i = k
                        continue
            i += 1
        return ex

    # -- 구조 도우미 -------------------------------------------------------
    def split_args(self, lo, hi):
        """토큰 [lo,hi) 를 최상위 쉼표로 나눈 (시작,끝) 목록(빈 인자 제외)."""
        out = []
        start = lo
        i = lo
        while i < hi:
            t = self.toks[i]
            if is_p(t, ",") :
                if i > start:
                    out.append((start, i))
                start = i + 1
                i += 1
                continue
            i = skip_group(self.toks, self.m, i)
        if hi > start:
            out.append((start, hi))
        return out

    def macro_name(self, p):
        """toks[p] 가 매크로 호출의 여는 괄호이면 (이름, 경로 앞 식별자들) 아니면 None."""
        t = self.toks
        if p >= 2 and is_p(t[p - 1], "!") and t[p - 2].k == "id":
            return t[p - 2].v
        return None

    def call_name(self, p):
        """toks[p] 가 함수/메서드 호출 여는 괄호이면 (이름, 메서드인가, 앞 경로)."""
        t = self.toks
        if p >= 1 and t[p - 1].k == "id" and is_p(t[p], "("):
            name = t[p - 1].v
            is_method = p >= 2 and is_p(t[p - 2], ".")
            path = []
            q = p - 2
            while q >= 1 and is_p(t[q], ":") and is_p(t[q - 1], ":"):
                if q >= 2 and t[q - 2].k == "id":
                    path.append(t[q - 2].v)
                    q -= 3
                else:
                    break
            return name, is_method, list(reversed(path))
        return None

    def ancestors(self, i):
        p = self.parent[i]
        while p >= 0:
            yield p
            p = self.parent[p]

    def excluded_context(self, i):
        """문자열 토큰 i 가 제외 범주(디버그·기계용·속성 등) 안인가. 범주 이름 또는 None."""
        t = self.toks
        n = len(t)
        # 비교·match 패턴 (`== "x"`, `"x" =>`, `"x" |`)
        if i >= 2 and is_p(t[i - 1], "=") and (is_p(t[i - 2], "=") or is_p(t[i - 2], "!")):
            return "패턴·비교"
        if i + 2 < n and is_p(t[i + 1], "=") and is_p(t[i + 2], ">"):
            return "패턴·비교"
        if i + 2 < n and is_p(t[i + 1], "|") and t[i + 2].k == "str":
            return "패턴·비교"
        if i >= 2 and is_p(t[i - 1], "|") and t[i - 2].k == "str":
            return "패턴·비교"
        if i + 2 < n and is_p(t[i + 1], "=") and is_p(t[i + 2], "="):
            return "패턴·비교"
        for p in self.ancestors(i):
            mn = self.macro_name(p)
            if mn in EXCLUDED_MACROS:
                return EXCLUDED_MACROS[mn]
            if is_p(t[p], "[") and p >= 1 and is_p(t[p - 1], "#"):
                return "속성(attr)"
            if is_p(t[p], "[") and p >= 2 and is_p(t[p - 1], "!") and is_p(t[p - 2], "#"):
                return "속성(attr)"
            cn = self.call_name(p)
            if cn:
                if cn[1] and cn[0] in EXCLUDED_METHODS:
                    return EXCLUDED_METHODS[cn[0]]
                if cn[0] in EXCLUDED_CALLS:
                    return EXCLUDED_CALLS[cn[0]]
                if not cn[1] and cn[0] == "new" and cn[2] and cn[2][-1] == "Command":
                    return "프로세스·환경"
        return None


    # -- 표현식 분류 -------------------------------------------------------
    def expr_text(self, lo, hi):
        """토큰 [lo,hi) 의 원문(공백 정리)."""
        if lo >= hi:
            return ""
        a = self.toks[lo].pos
        b = self.toks[hi].pos if hi < len(self.toks) else len(self.src)
        return re.sub(r"\s+", " ", self.src[a:b]).strip().rstrip(",").strip()

    def _path_prefix_len(self, lo, hi):
        """[lo,hi) 가 `a::b::` 형태 경로 접두이면 True (빈 범위 포함)."""
        t = self.toks
        i = lo
        while i < hi:
            if not (i + 2 < hi + 1 and t[i].k == "id" and i + 2 <= hi - 1
                    and is_p(t[i + 1], ":") and is_p(t[i + 2], ":")):
                return False
            i += 3
        return i == hi

    def classify_expr(self, lo, hi):
        """('text', 문자열토큰) | ('fmt', 매크로여는괄호) | None.
        `&`, `.to_string()`, `.into()`, `String::from(..)`, `Some(..)` 같은 포장은 벗긴다."""
        t = self.toks
        for _ in range(6):
            while lo < hi and is_p(t[lo], "&"):
                lo += 1
            # 뒤쪽 `.to_string()` 류
            if (hi - lo >= 5 and is_p(t[hi - 1], ")") and (hi - 1) in self.m
                    and self.m[hi - 1] == hi - 2 and t[hi - 3].k == "id"
                    and t[hi - 3].v in ("to_string", "into", "to_owned", "into_owned", "clone")
                    and is_p(t[hi - 4], ".")):
                hi -= 4
                continue
            # Some(..) / String::from(..) / Box::new(..)
            if hi - lo >= 3 and is_p(t[hi - 1], ")") and (hi - 1) in self.m:
                op = self.m[hi - 1]
                if (op > lo and t[op - 1].k == "id" and is_p(t[op], "(")
                        and not is_p(t[op - 1 - 1] if op >= 2 else t[op - 1], "!")):
                    name = t[op - 1].v
                    if name in ("Some", "from", "new") and self._path_prefix_len(lo, op - 1 - (3 if name != "Some" else 0) + (0 if name != "Some" else 0)) is not None:
                        if name == "Some" and op - 1 == lo:
                            lo, hi = op + 1, hi - 1
                            continue
                        if name in ("from", "new") and op - 4 >= lo - 0 and is_p(t[op - 2], ":") and is_p(t[op - 3], ":") and t[op - 4].k == "id" and t[op - 4].v in ("String", "Box", "Cow") and op - 4 == lo:
                            lo, hi = op + 1, hi - 1
                            continue
            break
        if hi - lo == 1 and t[lo].k == "str":
            return ("text", lo)
        if hi - lo >= 4 and is_p(t[hi - 1], ")") and (hi - 1) in self.m:
            op = self.m[hi - 1]
            mn = self.macro_name(op)
            if mn in ("format", "anyhow") and self._path_prefix_len(lo, op - 2):
                return ("fmt", op)
        return None

    def block_tail(self, open_idx):
        """`{ … }` 블록의 꼬리 식 범위 (마지막 최상위 `;` 뒤)."""
        t = self.toks
        blo, bhi = open_idx + 1, self.m[open_idx]
        last = blo - 1
        k = blo
        while k < bhi:
            if is_p(t[k], ";"):
                last = k
            k = skip_group(t, self.m, k)
        return last + 1, bhi

    def tail_ranges(self, lo, hi, depth=0):
        """식 [lo,hi) 의 꼬리 후보 범위들: 블록·if/else·match 의 각 갈래 끝 식까지 펼친다."""
        t = self.toks
        if lo >= hi or depth > 8:
            return []
        first = t[lo]
        out = []
        if is_id(first, "if") or is_id(first, "else"):
            k = lo
            while k < hi:
                if is_p(t[k], "{") and k in self.m:
                    a, b = self.block_tail(k)
                    out.extend(self.tail_ranges(a, b, depth + 1))
                    k = self.m[k] + 1
                else:
                    k = skip_group(t, self.m, k)
            return out
        if is_id(first, "match"):
            k = lo
            while k < hi and not (is_p(t[k], "{") and k in self.m):
                k = skip_group(t, self.m, k)
            if k >= hi:
                return []
            body_lo, body_hi = k + 1, self.m[k]
            j = body_lo
            while j < body_hi:
                if is_p(t[j], "=") and j + 1 < body_hi and is_p(t[j + 1], ">"):
                    s = j + 2
                    if s < body_hi and is_p(t[s], "{") and s in self.m:
                        a, b = self.block_tail(s)
                        out.extend(self.tail_ranges(a, b, depth + 1))
                        j = self.m[s] + 1
                    else:
                        e = s
                        while e < body_hi and not is_p(t[e], ","):
                            e = skip_group(t, self.m, e)
                        out.extend(self.tail_ranges(s, e, depth + 1))
                        j = e + 1
                else:
                    j = skip_group(t, self.m, j)
            return out
        if is_p(first, "{") and self.m.get(lo) == hi - 1:
            a, b = self.block_tail(lo)
            return self.tail_ranges(a, b, depth + 1)
        return [(lo, hi)]

    def closure_body(self, lo, hi):
        """클로저 인자의 본문 범위. 클로저가 아니면 그대로."""
        t = self.toks
        i = lo
        if i < hi and is_id(t[i], "move"):
            i += 1
        if i < hi and is_p(t[i], "|"):
            if i + 1 < hi and is_p(t[i + 1], "|"):
                return i + 2, hi
            j = i + 1
            while j < hi and not is_p(t[j], "|"):
                j = skip_group(t, self.m, j)
            return j + 1, hi
        return lo, hi

    # -- 서식 문자열 → 후보 -------------------------------------------------
    def value_exprs(self, args):
        """서식 문자열 뒤 인자들 → (위치 인자 식 목록, 이름 인자 사전)."""
        pos = []
        named = {}
        t = self.toks
        for (a, b) in args:
            if (b - a >= 3 and t[a].k == "id" and is_p(t[a + 1], "=")
                    and not is_p(t[a + 2], "=")):
                named[t[a].v] = self.expr_text(a + 2, b)
            else:
                pos.append(self.expr_text(a, b))
        return pos, named

    def fmt_cands(self, si, pos, named, rule, prefix=""):
        """문자열 토큰 si 를 서식 문자열로 보고 줄별 후보를 만든다."""
        tok = self.toks[si]
        s = tok.v
        out = []
        try:
            phs = parse_placeholders(s)
            ok = True
        except FmtError:
            phs = []
            ok = False
        exprs = {}
        counter = 0
        for p in phs:
            a = p.arg
            if a == "":
                e = pos[counter] if counter < len(pos) else ""
                counter += 1
            elif a.isdigit():
                k = int(a)
                e = pos[k] if k < len(pos) else ""
            else:
                e = named.get(a, "")
            exprs[id(p)] = e
        off = 0
        for li, ln in enumerate(s.split("\n")):
            lo, hi = off, off + len(ln)
            off = hi + 1
            src_line = tok.lines[li] if tok.lines and li < len(tok.lines) else tok.line
            core = ln.strip()
            lead = len(ln) - len(ln.lstrip())
            rel = []
            for p in phs:
                if p.start >= lo and p.end <= hi:
                    a = p.start - lo - lead
                    b = p.end - lo - lead
                    if 0 <= a and b <= len(core):
                        rel.append((a, b, p))
            if ok and rel:
                groups = []
                for (a, b, p) in rel:
                    if groups and groups[-1][-1][1] == a:
                        groups[-1].append((a, b, p))
                    else:
                        groups.append([(a, b, p)])
                alpha = letters("".join(
                    core[x:y] for x, y in _complement(core, [(a, b) for a, b, _ in rel])))
                if alpha < MIN_LITERAL_ALPHA:
                    continue
                lit_len = len("".join(
                    core[x:y] for x, y in _complement(core, [(a, b) for a, b, _ in rel])))
                if lit_len < MIN_OPEN_LITERAL_LEN and open_end_of(
                        core, [(a, b, p.arg) for a, b, p in rel]):
                    continue
                vals = []
                for gi, g in enumerate(groups, 1):
                    desc = ""
                    for (_a, _b, p) in g:
                        if p.arg == "" or p.arg.isdigit():
                            desc += "<%s>" % (exprs.get(id(p), "") or "?")
                        else:
                            desc += p.raw
                    vals.append("{%d}=%s" % (gi, desc))
                out.append(Cand(self.rel, src_line, "fmt", prefix + core, rule,
                                "  ".join(vals), si))
            else:
                txt = core
                if ok:
                    txt = txt.replace("{{", "{").replace("}}", "}")
                if letters(txt) < MIN_LITERAL_ALPHA:
                    continue
                out.append(Cand(self.rel, src_line, "text", prefix + txt, rule, "", si))
        return out

    def text_cands(self, si, rule, prefix="", min_alpha=MIN_LITERAL_ALPHA):
        tok = self.toks[si]
        out = []
        for li, ln in enumerate(tok.v.split("\n")):
            core = ln.strip()
            if letters(core) < min_alpha:
                continue
            src_line = tok.lines[li] if tok.lines and li < len(tok.lines) else tok.line
            out.append(Cand(self.rel, src_line, "text", prefix + core, rule, "", si))
        return out

    def group_fmt(self, p, rule, prefix="", index=0, scan_first_str=False):
        """매크로 괄호 p 안의 서식 문자열(index 번째 인자, 또는 첫 단일 문자열 인자)."""
        t = self.toks
        args = self.split_args(p + 1, self.m[p])
        fi = None
        if scan_first_str:
            for k, (a, b) in enumerate(args):
                if b - a == 1 and t[a].k == "str":
                    fi = k
                    break
        elif index < len(args):
            a, b = args[index]
            if b - a == 1 and t[a].k == "str":
                fi = index
        if fi is None:
            return [], None
        si = args[fi][0]
        pos, named = self.value_exprs(args[fi + 1:])
        return self.fmt_cands(si, pos, named, rule, prefix), si

    def functions(self):
        """(함수 이름, 본문 여는 중괄호, 닫는 중괄호) 목록."""
        t = self.toks
        out = []
        for i, tk in enumerate(t):
            if is_id(tk, "fn") and i + 1 < len(t) and t[i + 1].k == "id":
                k = i + 2
                while k < len(t) and not is_p(t[k], ";") and not (is_p(t[k], "{") and k in self.m):
                    k = skip_group(t, self.m, k)
                if k < len(t) and is_p(t[k], "{"):
                    out.append((t[i + 1].v, k, self.m[k]))
        return out

    def extract(self, variants):
        """(후보 목록, 처리한 문자열 토큰 집합)."""
        t = self.toks
        cands = []
        claimed = set()
        n = len(t)

        def add(cs, si):
            if si is not None:
                claimed.add(si)
            cands.extend(cs)

        def classified(lo, hi, rule, prefix="", text_ok=True, tails=False):
            ranges = self.tail_ranges(lo, hi) if tails else [(lo, hi)]
            for (a, b) in ranges:
                c = self.classify_expr(a, b)
                if not c:
                    continue
                kind, idx = c
                if kind == "fmt":
                    cs, si = self.group_fmt(idx, rule, prefix, scan_first_str=True)
                    add(cs, si)
                elif text_ok:
                    add(self.text_cands(idx, rule, prefix), idx)

        for i in range(n):
            if self.excluded[i]:
                continue
            tk = t[i]
            # #[error("…")]
            if is_p(tk, "#") and i + 3 < n and is_p(t[i + 1], "[") and (i + 1) in self.m:
                if is_id(t[i + 2], "error") and is_p(t[i + 3], "(") and (i + 3) in self.m:
                    args = self.split_args(i + 4, self.m[i + 3])
                    for (a, b) in args:
                        if b - a == 1 and t[a].k == "str":
                            pos, named = self.value_exprs(args[1:])
                            add(self.fmt_cands(a, pos, named, "#[error]"), a)
                            break
                continue
            # const NAME: &str = "…";
            if (is_id(tk, "const") or is_id(tk, "static")) and i + 2 < n and t[i + 1].k == "id":
                k = i + 2
                seen_str = False
                while k < n and not is_p(t[k], "=") and not is_p(t[k], ";"):
                    if is_id(t[k], "str"):
                        seen_str = True
                    k = skip_group(t, self.m, k)
                if seen_str and k < n and is_p(t[k], "="):
                    e = k + 1
                    while e < n and not is_p(t[e], ";"):
                        e = skip_group(t, self.m, e)
                    if CONST_MSG_NAME.search(t[i + 1].v):
                        classified(k + 1, e, "const")
                continue
            if not (tk.k == "p" and tk.v in OPEN and i in self.m):
                continue
            end = self.m[i]
            mn = self.macro_name(i)
            if mn in LOG_MACROS:
                cs, si = self.group_fmt(i, "log:" + mn, scan_first_str=True)
                add(cs, si)
            elif mn in BAIL_MACROS:
                cs, si = self.group_fmt(i, mn, index=0)
                add(cs, si)
            elif mn == "ensure":
                cs, si = self.group_fmt(i, mn, index=1)
                add(cs, si)
            elif mn in PRINT_MACROS and self.rel in PRINT_HUMAN_FILES:
                cs, si = self.group_fmt(i, mn, index=0)
                add(cs, si)
            elif mn in WRITE_MACROS and self.rel in WRITE_HUMAN_FILES:
                cs, si = self.group_fmt(i, mn, index=1)
                add(cs, si)
            elif mn == "format" and self.rel in FORMAT_HUMAN_FILES:
                cs, si = self.group_fmt(i, mn, index=0)
                add(cs, si)
            elif mn is None and is_p(tk, "{"):
                # 구조체 리터럴의 사람용 필드
                for (a, b) in self.split_args(i + 1, end):
                    if (b - a >= 3 and t[a].k == "id" and t[a].v in HUMAN_FIELDS
                            and is_p(t[a + 1], ":") and not is_p(t[a + 2], ":")):
                        classified(a + 2, b, "field:" + t[a].v)
            elif mn is None and is_p(tk, "("):
                cn = self.call_name(i)
                if not cn:
                    continue
                name, is_method, path = cn
                args = self.split_args(i + 1, end)
                if is_method and name in CLOSURE_METHODS and args:
                    lo, hi = self.closure_body(*args[0])
                    classified(lo, hi, name, tails=True)
                elif name == "msg" and path and path[-1] == "Error" and args:
                    classified(args[0][0], args[0][1], "Error::msg")
                elif name == "other" and path and path[-1] == "Error" and args:
                    classified(args[0][0], args[0][1], "Error::other")
                elif name == "new" and path and path[-1] == "Error" and len(args) == 2:
                    classified(args[1][0], args[1][1], "Error::new")
                elif name == "raw" and path and path[-1] == "Error" and len(args) >= 2:
                    classified(args[1][0], args[1][1], "clap::Error::raw", prefix="error: ")
                elif name == "from" and path and path[-1] == "Body" and args:
                    classified(args[0][0], args[0][1], "Body::from")
                elif name == "Err" and not is_method and not path and len(args) == 1:
                    classified(args[0][0], args[0][1], "Err")
                elif name in T_FNS and not is_method and len(args) == 1:
                    a, b = args[0]
                    if b - a == 1 and t[a].k == "str" and (not path or path[-1] == "i18n"):
                        if name == "t" and t[a].v != t[a].v.strip():
                            self.errors.append((self.rel, t[a].line, "t() 인자에 앞뒤 공백이 있습니다(공백 있는 라벨은 t_pad): %r" % t[a].v))
                        add(self.text_cands(a, "i18n::" + name, min_alpha=1), a)
                elif name in TR_FNS and not is_method and len(args) == 1:
                    classified(args[0][0], args[0][1], "i18n::" + name)
                elif name in STATUS_FNS and is_method and args:
                    c = self.classify_expr(*args[0])
                    if c and c[0] == "fmt":
                        cs, si = self.group_fmt(c[1], name, scan_first_str=True)
                        if si is not None:
                            claimed.add(si)
                            head = re.split(r"\{", t[si].v, 1)[0].strip().rstrip(":").strip()
                            if head:
                                cands.append(Cand(self.rel, t[si].line, "text", head,
                                                  name, "", si))
                elif not is_method and path and args:
                    key = (path[-1], name)
                    if key in variants or (path[-1] == "Self" and name in variants["__names__"]):
                        classified(args[0][0], args[0][1], "variant:%s::%s" % key)
        # 호출 위치로 잡을 수 없는 값 문자열
        for i, tk in enumerate(t):
            if tk.k != "str" or self.excluded[i] or i in claimed:
                continue
            for (f, pref) in EXTRA_SITES:
                if f == self.rel and tk.v.lstrip().startswith(pref):
                    self._site(i, "site", cands, claimed)
        return cands, claimed

    def _site(self, si, rule, cands, claimed, min_alpha=MIN_LITERAL_ALPHA):
        """문자열 토큰 si 를 둘러싼 서식 매크로가 있으면 서식으로, 아니면 평문으로."""
        t = self.toks
        p = self.parent[si]
        if p >= 0 and self.macro_name(p) in FORMAT_MACROS:
            cs, s2 = self.group_fmt(p, rule, scan_first_str=True)
            if s2 == si:
                claimed.add(si)
                cands.extend(cs)
                return
        claimed.add(si)
        cands.extend(self.text_cands(si, rule, min_alpha=min_alpha))

    def extract_modes(self, claimed):
        """파일(·함수) 단위 '전부 화면 문구' 모드. (후보 목록, 새로 처리한 토큰 집합)"""
        t = self.toks
        cands = []
        done = set()
        for (f, fre, min_alpha, need_space) in ALL_MODES:
            if f != self.rel:
                continue
            if fre is None:
                ranges = [(0, len(t))]
            else:
                rx = re.compile(fre)
                ranges = [(b0, b1) for (nm, b0, b1) in self.functions() if rx.search(nm)]
            for (lo, hi) in ranges:
                for i in range(lo, hi):
                    tk = t[i]
                    if tk.k != "str" or self.excluded[i] or i in claimed:
                        continue
                    if need_space and " " not in tk.v:
                        continue
                    if letters(re.sub(r"\{[^{}]*\}", "", tk.v)) < min_alpha:
                        continue
                    if ALL_IGNORE.search(tk.v.strip()) or self.excluded_context(i):
                        continue
                    self._site(i, "mode:" + f, cands, done, min_alpha)
        return cands, done


def _complement(s, spans):
    """문자열 s 에서 spans(정렬된 (a,b)) 바깥 구간 목록."""
    out = []
    last = 0
    for a, b in sorted(spans):
        out.append((last, a))
        last = b
    out.append((last, len(s)))
    return out


def collect_variants(analyses):
    """`#[error("{0}")]` 를 가진 열거형 변형 -> {(열거형, 변형)} (+ "__names__" 변형 이름 집합)."""
    out = {}
    names = set()
    for an in analyses:
        t = an.toks
        n = len(t)
        for i in range(n - 8):
            if not (is_p(t[i], "#") and is_p(t[i + 1], "[") and is_id(t[i + 2], "error")
                    and is_p(t[i + 3], "(") and t[i + 4].k == "str" and t[i + 4].v == "{0}"
                    and is_p(t[i + 5], ")") and is_p(t[i + 6], "]")):
                continue
            j = i + 7
            while j < n and is_p(t[j], "#") and j + 1 < n and is_p(t[j + 1], "["):
                j = an.m.get(j + 1, j + 1) + 1
            if j >= n or t[j].k != "id":
                continue
            p = an.parent[i]
            if p >= 2 and is_id(t[p - 2], "enum"):
                out[(t[p - 1].v, t[j].v)] = True
                names.add(t[j].v)
    out["__names__"] = names
    return out


# ---------------------------------------------------------------------------
# 4. 전체 스캔과 재현율 게이트
# ---------------------------------------------------------------------------

SKIP_DIRS = {"i18n"}
SKIP_FILES = {"test_harnesses.rs"}


def source_files(src_root=None):
    root = src_root or SRC
    out = []
    for d, dirs, fs in os.walk(root):
        rel_d = os.path.relpath(d, root).replace(os.sep, "/")
        if rel_d == ".":
            rel_d = ""
        dirs[:] = sorted(x for x in dirs if not (rel_d == "" and x in SKIP_DIRS))
        for f in sorted(fs):
            if f.endswith(".rs") and not (rel_d == "" and f in SKIP_FILES):
                out.append(((rel_d + "/" if rel_d else "") + f, os.path.join(d, f)))
    return sorted(out)


def read_text(path):
    with open(path, encoding="utf-8", newline="") as fh:
        return fh.read().replace("\r\n", "\n")


def analyse_tree(src_root=None):
    ans = []
    for rel, path in source_files(src_root):
        ans.append(Analysis(rel, read_text(path)))
    return ans


class ScanResult:
    def __init__(self):
        self.cands = []
        self.dropped = []   # 허용목록(파일:접두)으로 후보에서 뺀 것
        self.analyses = []
        self.claimed = {}   # rel -> set(str token idx)
        self.errors = []    # (파일, 줄, 메시지)


def scan_tree(src_root=None, allow=None):
    """소스 전체에서 후보를 뽑는다. 허용목록(allow)에 맞는 후보는 화면 메시지가 아니므로 뺀다."""
    res = ScanResult()
    res.analyses = analyse_tree(src_root)
    variants = collect_variants(res.analyses)
    seen = set()
    for an in res.analyses:
        cs, claimed = an.extract(variants)
        res.errors.extend(an.errors)
        ms, done = an.extract_modes(claimed)
        cs.extend(ms)
        claimed |= done
        for c in cs:
            k = (c.file, c.line, c.kind, c.en)
            if k in seen:
                continue
            seen.add(k)
            # 후보 제거는 `파일:접두` 항목만 가능하다(정규식·파일 단위 항목은 게이트에만 적용 —
            # 확정된 사람용 규칙(log/bail/context 등)의 후보를 조용히 지우지 않기 위해)
            if allow and any(e.kind == "prefix" and _allow_hit(e, c.file, c.en) for e in allow):
                res.dropped.append(c)
                continue
            res.cands.append(c)
        res.claimed[an.rel] = claimed
    return res


def _allow_hit(entry, rel, value):
    if entry.matches(rel, value):
        entry.hits += 1
        return True
    return False


def qualifies(value):
    """재현율 게이트 대상: 알파벳 4자 이상 + 공백 포함(자리표시자 제외 기준)."""
    if " " not in value:
        return False
    stripped = re.sub(r"\{[^{}]*\}", "", value)
    return letters(stripped) >= MIN_LITERAL_ALPHA


# -- 허용목록 ----------------------------------------------------------------

class AllowEntry:
    def __init__(self, kind, file, pat, reason, lineno):
        self.kind = kind      # file | re | prefix
        self.file = file      # "" 이면 모든 파일 (re 전용)
        self.pat = pat
        self.reason = reason
        self.category = reason.split(":", 1)[0].strip() if ":" in reason else (reason.strip() or "기타")
        self.lineno = lineno
        self.hits = 0
        self.rx = re.compile(pat) if kind == "re" else None

    def matches(self, rel, value):
        if self.kind == "file":
            return rel == self.file
        if self.kind == "re":
            return (not self.file or rel == self.file) and self.rx.search(value) is not None
        return rel == self.file and value.lstrip().startswith(self.pat)


def parse_allow(text):
    entries = []
    for no, raw in enumerate(text.split("\n"), 1):
        line = raw.rstrip()
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        reason = ""
        if " ## " in line:
            line, reason = line.split(" ## ", 1)
            reason = reason.strip()
        line = line.rstrip()
        if line.startswith("re:"):
            entries.append(AllowEntry("re", "", line[3:], reason, no))
        elif re.match(r"^[\w./-]+\.rs$", line):
            entries.append(AllowEntry("file", line, "", reason, no))
        else:
            m = re.match(r"^([\w./-]+\.rs):(.*)$", line)
            if not m:
                raise ValueError("허용목록 %d 줄 형식 오류: %r" % (no, raw))
            if m.group(2).startswith("re:"):
                entries.append(AllowEntry("re", m.group(1), m.group(2)[3:], reason, no))
            else:
                entries.append(AllowEntry("prefix", m.group(1), m.group(2), reason, no))
    return entries


def load_allow(path=None):
    path = path or ALLOW_FILE
    if not os.path.exists(path):
        return []
    return parse_allow(read_text(path))


def unclassified(res, allow):
    """(미분류 목록[(file,line,value)], 범주별 제외 건수)."""
    out = []
    excl = {}
    for an in res.analyses:
        claimed = res.claimed.get(an.rel, set())
        for i, tk in enumerate(an.toks):
            if tk.k != "str" or an.excluded[i] or i in claimed:
                continue
            if not qualifies(tk.v):
                continue
            why = an.excluded_context(i)
            if why:
                excl[why.split(":")[0]] = excl.get(why.split(":")[0], 0) + 1
                continue
            if any(_allow_hit(e, an.rel, tk.v) for e in allow):
                continue
            out.append((an.rel, tk.line, tk.v))
    return out, excl


# ---------------------------------------------------------------------------
# 5. 번역표 전용 파서·작성기 (plan §2.3 규칙 — tomllib 없이)
# ---------------------------------------------------------------------------

class Entry:
    """번역표 항목 하나."""

    def __init__(self, kind, en="", ko="", comments=None, line=0):
        self.kind = kind            # "text" | "fmt"
        self.en = en
        self.ko = ko
        self.comments = comments or []   # 항목 바로 위 주석 줄(# 포함)
        self.line = line
        self.has_en = False
        self.has_ko = False

    def meta(self, tag):
        """`# tag: 값` 주석의 값 (없으면 "")."""
        for c in self.comments:
            m = re.match(r"^#\s*" + re.escape(tag) + r"\s*:\s*(.*)$", c)
            if m:
                return m.group(1).strip()
        return ""


_VALUE_RE = re.compile(r'^(en|ko)\s*=\s*"((?:[^"\\]|\\.)*)"\s*(?:#.*)?$')


def unescape_basic(raw):
    """한 줄 기본 문자열 본문의 이스케이프 해제. 허용: \\" \\\\ \\n \\t \\r \\uXXXX."""
    out = []
    i = 0
    n = len(raw)
    while i < n:
        c = raw[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        e = raw[i + 1:i + 2]
        if e == '"':
            out.append('"')
        elif e == "\\":
            out.append("\\")
        elif e == "n":
            out.append("\n")
        elif e == "t":
            out.append("\t")
        elif e == "r":
            out.append("\r")
        elif e == "u":
            h = raw[i + 2:i + 6]
            if not re.match(r"^[0-9A-Fa-f]{4}$", h):
                raise ValueError("\\u 뒤에 16진수 4자리가 필요합니다")
            out.append(chr(int(h, 16)))
            i += 6
            continue
        else:
            raise ValueError("허용되지 않은 이스케이프 \\%s" % e)
        i += 2
    return "".join(out)


def escape_basic(s):
    out = []
    for ch in s:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\r":
            out.append("\\r")
        elif ord(ch) < 0x20 or ord(ch) == 0x7F:
            out.append("\\u%04X" % ord(ch))
        else:
            out.append(ch)
    return "".join(out)


def parse_table(src):
    """번역표 문자열 → (항목 목록, 오류 목록[(줄, 메시지)]). 파싱 규칙 위반은 오류로 모은다."""
    entries = []
    errors = []
    pending = []
    cur = None

    def close():
        nonlocal cur
        if cur is not None:
            if not cur.has_en:
                errors.append((cur.line, "en 필드가 없습니다"))
            if not cur.has_ko:
                errors.append((cur.line, "ko 필드가 없습니다"))
            entries.append(cur)
        cur = None

    for no, raw in enumerate(src.replace("\r\n", "\n").split("\n"), 1):
        line = raw.strip()
        if not line:
            pending = []
            continue
        if line.startswith("#"):
            pending.append(line)
            continue
        if line in ("[[text]]", "[[fmt]]"):
            close()
            cur = Entry("text" if line == "[[text]]" else "fmt", comments=pending, line=no)
            pending = []
            continue
        if line.startswith("["):
            close()
            errors.append((no, "허용되지 않은 표 머리 %s ([[text]] 와 [[fmt]] 만 허용)" % line))
            continue
        if line.startswith(('"""', "'''")) or '"""' in line:
            errors.append((no, '여러 줄 문자열("""...""")은 허용되지 않습니다'))
            continue
        m = _VALUE_RE.match(line)
        if not m:
            key = line.split("=", 1)[0].strip()
            if key not in ("en", "ko"):
                errors.append((no, "알 수 없는 필드 %r (en, ko 만 허용)" % key))
            elif re.match(r"^(en|ko)\s*=\s*'", line):
                errors.append((no, "리터럴 문자열('...')은 허용되지 않습니다"))
            else:
                errors.append((no, "한 줄 기본 문자열(\"...\")이 아닙니다"))
            continue
        if cur is None:
            errors.append((no, "[[text]]/[[fmt]] 머리 밖의 값"))
            continue
        field, body = m.group(1), m.group(2)
        try:
            val = unescape_basic(body)
        except ValueError as e:
            errors.append((no, str(e)))
            continue
        if field == "en":
            if cur.has_en:
                errors.append((no, "en 이 두 번 나옵니다"))
            cur.en, cur.has_en = val, True
        else:
            if cur.has_ko:
                errors.append((no, "ko 가 두 번 나옵니다"))
            cur.ko, cur.has_ko = val, True
    close()
    return entries, errors


def format_entry(e):
    lines = list(e.comments)
    lines.append("[[%s]]" % e.kind)
    lines.append('en = "%s"' % escape_basic(e.en))
    lines.append('ko = "%s"' % escape_basic(e.ko))
    return "\n".join(lines) + "\n"


def format_table(entries, header=""):
    parts = []
    if header:
        parts.append(header.rstrip("\n") + "\n")
    for e in entries:
        parts.append(format_entry(e))
    return "\n".join(parts)


DEFAULT_HEADER = """# dx CLI 한국어 번역표 (DX_LANG=en 이면 사용하지 않습니다)
#
# 형식과 규칙 요약
#  1. 표는 [[text]] 와 [[fmt]] 두 종류뿐이며 필드는 en, ko 둘뿐입니다.
#  2. 값은 한 줄 기본 문자열("...")만 씁니다. 줄바꿈은 \\n, 그 밖의 이스케이프는 \\" \\\\ \\t \\uXXXX 입니다.
#  3. [[text]]: en 은 화면에 나오는 문자열과 완전히 같아야 합니다. ko 에 {n} 은 쓰지 않습니다.
#  4. [[fmt]]: en 은 Rust 서식 문자열의 한 줄이며 자리표시자 1개 이상, 리터럴 알파벳 4자 이상입니다.
#     붙어 있는 자리표시자는 값 묶음 1개이며 ko 는 {1}..{N} 으로 참조하고 모든 묶음을 최소 1회 씁니다.
#  5. en 이 값 묶음으로 끝나면 ko 도 마지막 묶음 참조로, 시작하면 {1} 로 시작해야 합니다.
#  6. 같은 en 이 두 번 나오면 오류입니다.  7. ko = "" 는 미번역, ko = en 은 처리 완료입니다.
#  8. 주석: "# == 구역: ... ==" 머리, "# src: 파일:줄", "# 값: {1}=... {2}=..." (런타임은 무시).
"""


def leading_header(path):
    """기존 번역표 앞부분의 머리 주석(첫 `[[` 또는 `# == 구역` 앞까지)."""
    if not os.path.exists(path):
        return ""
    out = []
    for raw in read_text(path).split("\n"):
        if raw.startswith("[[") or raw.startswith("# == 구역"):
            break
        out.append(raw)
    while out and not out[-1].strip():
        out.pop()
    return "\n".join(out) + "\n" if out else ""


# ---------------------------------------------------------------------------
# 6. 키 모으기 (소스 후보 + 도움말 덤프)
# ---------------------------------------------------------------------------

class Key:
    def __init__(self, en, kind, zone):
        self.en = en
        self.kind = kind
        self.zone = zone
        self.where = []       # (파일, 줄)
        self.vals = ""
        self.meta = []        # 덤프 항목의 kind/path 주석
        self.from_dump = False


# 소스 리터럴이 아니라 런타임이 직접 조회하는 합성 키: (en, 종류, 구역, (파일, 줄))
RUNTIME_KEYS = [
    ("Caused by:", "text", "T5", ("i18n/mod.rs", "tr_error_report")),   # anyhow 보고서 머리(함수 이름 기준)
    # cargo_metadata 0.19.2 src/errors.rs 의 사용자 노출 문구(외부 크레이트라 소스 추출 대상이 아님)
    ("`cargo metadata` exited with an error: {stderr}", "fmt", "T5", ("cargo_metadata-0.19.2/errors.rs", "Error::CargoMetadata")),
    ("failed to start `cargo metadata`: {0}", "fmt", "T5", ("cargo_metadata-0.19.2/errors.rs", "Error::Io")),
    ("cannot convert the stdout of `cargo metadata`: {0}", "fmt", "T5", ("cargo_metadata-0.19.2/errors.rs", "Error::Utf8")),
    ("cannot convert the stderr of `cargo metadata`: {0}", "fmt", "T5", ("cargo_metadata-0.19.2/errors.rs", "Error::ErrUtf8")),
    ("failed to interpret `cargo metadata`'s json: {0}", "fmt", "T5", ("cargo_metadata-0.19.2/errors.rs", "Error::Json")),
    ("could not find any json in the output of `cargo metadata`", "text", "T5", ("cargo_metadata-0.19.2/errors.rs", "Error::NoJson")),
    # serve/output.rs 의 토글 메시지: 소스는 "{} is now {}" 꼴로 on/off 를 값으로 끼우지만 TUI 에는 줄 전체를 합성해 보낸다.
    ("Verbose logging is now on", "text", "T5", ("serve/output.rs", "Verbose/Tracing toggle")),
    ("Verbose logging is now off", "text", "T5", ("serve/output.rs", "Verbose/Tracing toggle")),
    ("Tracing is now on", "text", "T5", ("serve/output.rs", "Verbose/Tracing toggle")),
    ("Tracing is now off", "text", "T5", ("serve/output.rs", "Verbose/Tracing toggle")),
]


def collect_keys(cands, dump_entries=None):
    """(키 사전 en→Key, 경고 목록). 같은 키가 여러 구역이면 번호가 가장 작은 구역 소유."""
    keys = {}
    warns = []
    for (en, kind, zone, where) in RUNTIME_KEYS:
        k = Key(en, kind, zone)
        k.where.append(where)
        keys[en] = k
    for e in dump_entries or []:
        k = keys.get(e.en)
        if k is None:
            k = Key(e.en, e.kind, "T1")
            k.from_dump = True
            k.meta = [c for c in e.comments if re.match(r"^#\s*(kind|path)\s*:", c)]
            keys[e.en] = k
        elif k.kind != e.kind:
            warns.append("덤프에서 같은 en 이 종류가 다릅니다: %r" % e.en)
    for c in cands:
        k = keys.get(c.en)
        if k is None:
            k = Key(c.en, c.kind, c.zone)
            keys[c.en] = k
        else:
            if k.kind != c.kind:
                warns.append("같은 en 이 [[text]] 와 [[fmt]] 로 모두 나옵니다(앞의 것 유지): %r (%s:%d)"
                             % (c.en, c.file, c.line))
                continue
            if ZONES.index(c.zone) < ZONES.index(k.zone):
                k.zone = c.zone
        k.where.append((c.file, c.line))
        if c.vals and not k.vals:
            k.vals = c.vals
    return keys, warns


def read_dump(path):
    """도움말 키 덤프 → 항목 목록. kind 주석 값을 검증한다."""
    if not os.path.isfile(path):
        err("도움말 덤프를 읽을 수 없습니다: %s" % path)
        raise SystemExit(2)
    entries, errors = parse_table(read_text(path))
    if errors:
        raise SystemExit("도움말 덤프 %s 파싱 오류: %s" % (path, "; ".join("%d: %s" % x for x in errors[:5])))
    for e in entries:
        kd = e.meta("kind")
        if kd and kd not in DUMP_KINDS:
            raise SystemExit("도움말 덤프 %s: 알 수 없는 kind %r (줄 %d)" % (path, kd, e.line))
    return entries


# ---------------------------------------------------------------------------
# 7. 검사 (plan §5.1 검사 1~8 + F15 + F24)
# ---------------------------------------------------------------------------

# (영어 정규식, 한국어에 있어야 하는 표현 정규식, 이름) — glossary.md 의 합의 용어와 승인된 추가 용어
# 4번째 항목(선택)은 영어 문장에 이 정규식이 있을 때만 검사하는 문맥 한정이다(glossary.md 가
# `target (Rust target triple)`, `feature (cargo)` 로 범위를 한정한 용어).
# 5번째 항목(선택)은 제외 문맥 정규식이다: 영어에서 이 부분을 뺀 나머지에서만 용어를 찾는다
# (예: `version component` 는 컴포넌트가 아니라 구성 요소). 어떤 용어에도 쓸 수 있다.
# 항목 주석 `# glossary-ok: <영어 단어>` 로 그 항목만 해당 용어 검사를 건너뛸 수도 있다(보조).
# glossary.md 와의 차이: app→앱, project→프로젝트, package→패키지, compile→컴파일, library→라이브러리,
# binary→바이너리 등 일반어는 오탐이 많아 검사하지 않는다(번역자가 용어집 표를 직접 따른다).
# 반대로 여기에만 있는 승인 추가 용어: manifest, proxy, template, component, registry, toolchain,
# simulator, device, directory, Usage, possible values.
GLOSSARY = [
    (r"hot[- ]?re-?load(?:ing|ed|s)?|hot reload", r"핫 ?리로드", "hot-reload → 핫 리로드"),
    (r"hot[- ]?patch(?:ing|ed|es)?", r"핫 ?패치", "hot-patch → 핫 패치"),
    (r"bundl(?:e|es|ed|ing)", r"번들", "bundle → 번들"),
    (r"assets?", r"에셋", "asset → 에셋"),
    (r"targets?", r"타깃", "target → 타깃", r"triple|cargo|rust|--target|compil|toolchain", None),
    (r"crates?", r"크레이트", "crate → 크레이트"),
    (r"features?", r"피처", "feature → 피처", r"cargo|crate|flag|--features|manifest|Cargo\.toml", None),
    (r"workspaces?", r"워크스페이스", "workspace → 워크스페이스"),
    (r"telemetry", r"텔레메트리", "telemetry → 텔레메트리"),
    (r"builds?|building|built", r"빌드", "build → 빌드"),
    (r"renderers?", r"렌더러", "renderer → 렌더러"),
    (r"platforms?", r"플랫폼", "platform → 플랫폼"),
    (r"fullstack", r"풀스택", "fullstack → 풀스택"),
    (r"releases?", r"릴리스|릴리즈", "release → 릴리스"),
    (r"profiles?", r"프로필|프로파일", "profile → 프로필"),
    (r"linkers?", r"링커", "linker → 링커"),
    (r"manifests?", r"매니페스트", "manifest → 매니페스트"),
    (r"prox(?:y|ies)", r"프록시", "proxy → 프록시"),
    (r"templates?", r"템플릿", "template → 템플릿"),
    (r"components?", r"컴포넌트", "component → 컴포넌트", None, r"(?i)\b(?:version|file|path) components?\b"),
    (r"registry|registries", r"레지스트리", "registry → 레지스트리"),
    (r"toolchains?", r"툴체인", "toolchain → 툴체인"),
    (r"simulators?", r"시뮬레이터", "simulator → 시뮬레이터"),
    (r"devices?", r"기기", "device → 기기"),
    (r"dependenc(?:y|ies)", r"의존성", "dependency → 의존성"),
    (r"directory|directories", r"디렉터리", "directory → 디렉터리"),
    (r"usage", r"사용법", "Usage → 사용법"),
    (r"possible values", r"가능한 값", "possible values → 가능한 값"),
]
GLOSSARY_BANNED = [(r"디렉토리", "디렉토리 → 디렉터리"), (r"릴리즈", "릴리즈 → 릴리스")]


def clean_for_glossary(en):
    """용어집 검사에서 제외할 부분(백틱 안, 옵션·경로·식별자 모양 토큰, 자리표시자)을 뺀다."""
    s = re.sub(r"`[^`]*`", " ", en)
    s = re.sub(r"\{[^{}]*\}", " ", s)
    s = re.sub(r"'[^']*'", lambda m: " " if re.search(r"[_./:]|(?<![A-Za-z])-", m.group(0)) else m.group(0), s)
    toks = []
    for t in re.split(r"\s+", s):
        tt = t.strip(",;:()\"'")
        # 하이픈 낱말(hot-reload 등 합의 용어)은 남긴다. `-` 로 시작하는 옵션과 식별자·경로 모양은 뺀다
        if re.match(r"^[A-Za-z]+(?:-[A-Za-z]+)*[.,;:!?)]*$", tt):
            toks.append(t)
            continue
        if tt.startswith("-") or re.search(r"[_/.:@=\[\]<>$]", tt):
            continue
        toks.append(t)
    return " ".join(toks)


def is_identity(e):
    """번역이 필요 없다고 표시한 항목(ko 가 en 과 같음; [[fmt]] 는 묶음을 {N} 으로 바꾼 것과 같음)."""
    if e.ko == e.en:
        return True
    if e.kind != "fmt":
        return False
    try:
        phs = parse_placeholders(e.en)
    except FmtError:
        return False
    out = []
    last = 0
    for gi, g in enumerate(group_placeholders(e.en, phs), 1):
        out.append(e.en[last:g[0].start])
        out.append("{%d}" % gi)
        last = g[-1].end
    out.append(e.en[last:])
    return "".join(out) == e.ko


def style_warnings(e):
    """문체 경고 (검사 7). 항등(ko == en)과 미번역은 대상 아님."""
    ko = e.ko
    out = []
    if not ko or is_identity(e):
        return out
    # F15 로 번역이 `…어요: {1}` 꼴로 끝날 수 있으므로, 끝쪽의 {N}·구두점·괄호·따옴표·백틱·콜론을 벗긴 뒤 어미를 본다
    body = ko
    while True:
        nb = re.sub(r"(?:\{\d+\}|[\s:;,\-–—()\[\]<>'\"`‘’“”…]|(?<=[.!?])[.!?])+$", "", body)
        if nb == body:
            break
        body = nb
    if re.search(r"(?<!필)(?<!중)(?<!개)(?<!수)(?<!주)요[.!?]?$", body):
        out.append("해요체로 보입니다(합니다체 사용): %r" % ko)
    elif re.search(r"[^니]다[.!?]?$", body) and not re.search(r"(니다|입니다)[.!?]?$", body):
        out.append("해라체로 보입니다(합니다체 사용): %r" % ko)
    en_dot = e.en.rstrip().endswith(".") and not e.en.rstrip().endswith("..")
    ko_dot = ko.rstrip().endswith(".") and not ko.rstrip().endswith("..")
    if e.en.rstrip().endswith("}") and ko.rstrip().endswith("}"):
        return out
    if en_dot != ko_dot:
        out.append("마침표가 원문과 다릅니다(원문 %s, 번역 %s): %r"
                   % ("있음" if en_dot else "없음", "있음" if ko_dot else "없음", ko))
    return out


def glossary_warnings(e):
    ko = e.ko
    out = []
    if not ko or is_identity(e):
        return out
    en = clean_for_glossary(e.en)
    ok_words = {w.lower() for w in re.split(r"[,\s]+", e.meta("glossary-ok")) if w}
    for (en_rx, ko_rx, name, *rest) in GLOSSARY:
        ctx_in = rest[0] if len(rest) > 0 else None
        ctx_out = rest[1] if len(rest) > 1 else None
        if name.split()[0].lower() in ok_words:
            continue
        if ctx_in and not re.search(ctx_in, e.en, re.I):   # 문맥 판정은 정리 전 원문으로
            continue
        target = re.sub(ctx_out, " ", en) if ctx_out else en
        if re.search(r"(?<![A-Za-z])(?:%s)(?![A-Za-z])" % en_rx, target, re.I) and not re.search(ko_rx, ko):
            out.append("용어집 위반 [%s]: %r" % (name, ko))
    for (rx, name) in GLOSSARY_BANNED:
        if re.search(rx, ko):
            out.append("용어집 위반 [%s]: %r" % (name, ko))
    return out


def fmt_errors(e):
    """[[fmt]] 항목의 오류 목록(검사 2 + F24)과 열린 끝 위반(F15) 목록. (errors, f15)"""
    errs = []
    f15 = []
    try:
        phs = parse_placeholders(e.en)
    except FmtError as ex:
        return ["en 서식 오류: %s" % ex], f15
    groups = group_placeholders(e.en, phs)
    if not groups:
        errs.append("자리표시자가 없습니다([[text]] 로 옮기십시오)")
        return errs, f15
    if literal_alpha_of(e.en, phs) < MIN_LITERAL_ALPHA:
        errs.append("리터럴 알파벳이 %d자 미만입니다" % MIN_LITERAL_ALPHA)
    elif literal_len_of(e.en, phs) < MIN_OPEN_LITERAL_LEN and fmt_is_open(e.en):
        errs.append("열린 끝(값 묶음으로 시작·끝) 서식은 리터럴이 공백·문장부호 포함 %d자 이상이어야 합니다" % MIN_OPEN_LITERAL_LEN)
    if e.ko == "":
        return errs, f15
    refs, kerrs = parse_ko_template(e.ko, len(groups))
    errs.extend("ko " + m + (" — 값 묶음은 {1}..{N} 번호로 적습니다" if "잘못된 참조" in m else "")
                for m in kerrs)
    used = {k for (_a, _b, k) in refs}
    missing = [k for k in range(1, len(groups) + 1) if k not in used]
    if missing:
        errs.append("사용하지 않은 값 묶음: " + ", ".join("{%d}" % k for k in missing))
    starts, ends = fmt_edges(e.en)
    if not kerrs:
        ko = e.ko
        if starts and not (refs and refs[0][0] == 0 and refs[0][2] == 1):
            f15.append("열린 끝 위반: 원문이 값 묶음으로 시작하므로 ko 도 {1} 로 시작해야 합니다")
        if ends and not (refs and refs[-1][1] == len(ko) and refs[-1][2] == len(groups)):
            f15.append("열린 끝 위반: 원문이 값 묶음으로 끝나므로 ko 도 {%d} 로 끝나야 합니다" % len(groups))
    return errs, f15


def check_entries(entries, strict_items=True):
    """항목 목록의 개별 검사. 반환: dict(errors, f15, style, glossary, text_braces, untranslated, dups)."""
    res = {"errors": [], "f15": [], "style": [], "glossary": [], "warn": [],
           "untranslated": [], "dups": []}
    seen = {}
    for e in entries:
        where = "줄 %d" % e.line
        if e.en in seen:
            res["dups"].append("%s: 중복 키(첫 줄 %d): %r" % (where, seen[e.en], e.en))
        else:
            seen[e.en] = e.line
        if e.en.strip() != e.en or not e.en:
            res["errors"].append("%s: en 의 앞뒤 공백/빈 값: %r" % (where, e.en))
        if e.kind == "fmt":
            errs, f15 = fmt_errors(e)
            res["errors"].extend("%s: %s: %r" % (where, m, e.en) for m in errs)
            res["f15"].extend("%s: %s: %r" % (where, m, e.en) for m in f15)
        else:
            if re.search(r"\{\d+\}", e.ko):
                res["warn"].append("%s: [[text]] 의 ko 에 {숫자} 가 있습니다: %r" % (where, e.ko))
        if e.ko == "":
            res["untranslated"].append("%s: %r" % (where, e.en))
            continue
        res["style"].extend("%s: %s" % (where, m) for m in style_warnings(e))
        res["glossary"].extend("%s: %s" % (where, m) for m in glossary_warnings(e))
    return res


# ---------------------------------------------------------------------------
# 8. 명령
# ---------------------------------------------------------------------------

def err(*a):
    print(*a, file=sys.stderr)


def one_line(v, limit=110):
    s = json.dumps(v, ensure_ascii=False)
    return s if len(s) <= limit else s[:limit - 1] + "…"


def show_list(title, items, limit=40):
    if not items:
        return
    print("%s (%d)" % (title, len(items)))
    for it in items[:limit]:
        print("  - " + it)
    if len(items) > limit:
        print("  ... 외 %d건" % (len(items) - limit))


def cmd_scan(args):
    allow = load_allow()
    res = scan_tree(allow=allow)
    for (f, l, m) in sorted(set(res.errors)):
        err("오류: %s:%d: %s" % (f, l, m))
    if args.unclassified:
        un, excl = unclassified(res, allow)
        for (f, l, v) in un:
            print("%s:%d: %s" % (f, l, one_line(v)))
        err("재현율 게이트: 미분류 %d건, 제외 범주 %s" % (
            len(un), ", ".join("%s %d" % (k, v) for k, v in sorted(excl.items())) or "없음"))
        cats = {}
        for e in allow:
            cats.setdefault(e.category, []).append(e)
        for c in res.dropped:
            err("허용목록으로 후보에서 뺌: %s:%d %s" % (c.file, c.line, one_line(c.en, 70)))
        err("허용목록 %d항목: %s" % (len(allow), ", ".join(
            "%s %d(적중 %d)" % (c, len(es), sum(1 for x in es if x.hits)) for c, es in sorted(cats.items()))))
        unused = [e for e in allow if e.hits == 0]
        if unused:
            err("적중 0건 허용 항목 %d개(낡았을 수 있음): %s" % (
                len(unused), "; ".join("줄 %d" % e.lineno for e in unused)))
        return 1 if (un or res.errors) else 0
    if args.json:
        out = []
        for c in res.cands:
            d = c.as_dict()
            d["vals"] = c.vals
            out.append(d)
        json.dump(out, sys.stdout, ensure_ascii=False, indent=1)
        print()
        return 0
    keys, warns = collect_keys(res.cands)
    for c in res.cands:
        print("%s:%d\t%s\t%s\t%s" % (c.file, c.line, c.zone, c.kind, one_line(c.en, 140)))
    zc = {}
    for k in keys.values():
        zc[k.zone] = zc.get(k.zone, 0) + 1
    err("후보 %d건, 고유 키 %d개 (%s)" % (len(res.cands), len(keys), ", ".join(
        "%s %d" % (z, zc.get(z, 0)) for z in ZONES[1:])))
    for w in warns:
        err("경고: " + w)
    return 1 if res.errors else 0


_PART_RX = re.compile(r"^ko\.(T\d)(?:\.p(\d+))?\.toml$")


def part_key_of_filename(path):
    """조각 파일 이름 ko.Tn.toml / ko.Tn.pM.toml → (구역, 조각 번호). 아니면 None."""
    m = _PART_RX.match(os.path.basename(path))
    return (m.group(1), int(m.group(2) or 0)) if m else None


def _where_key(w):
    return (w[0], w[1] if isinstance(w[1], int) else 0, str(w[1]))


def _fmt_where(w):
    return ("%s:%d" if isinstance(w[1], int) else "%s:%s") % w


def load_prior(out_dir):
    """이미 번역된 항목 en -> (종류, ko). DIR 의 구역 파일이 ko.toml 보다 먼저."""
    prior = {}
    bad = []   # (파일, 파싱 오류) — 있으면 skeleton 이 덮어쓰기를 중단한다

    def feed(path):
        if not os.path.exists(path):
            return
        entries, perrs = parse_table(read_text(path))
        if perrs:
            bad.append((path, perrs))
        for e in entries:
            if e.ko != "" and e.en not in prior:
                prior[e.en] = (e.kind, e.ko, e.meta("glossary-ok"))

    if os.path.isdir(out_dir):
        names = [(part_key_of_filename(n), n) for n in os.listdir(out_dir)]
        for (key, name) in sorted(x for x in names if x[0] is not None):
            feed(os.path.join(out_dir, name))
    feed(KO_TOML)
    return prior, bad


def _own_first(k, z):
    own = [w for w in k.where if zone_of(w[0]) == z]
    return min(own, key=_where_key) if own else (min(k.where, key=_where_key) if k.where else ("", 0))


def zone_header(z):
    return "# == 구역: %s — %s ==\n" % (z, ZONE_TITLE[z])


def cmd_skeleton(args):
    allow = load_allow()
    res = scan_tree(allow=allow)
    dump = read_dump(args.help_keys) if args.help_keys else None
    keys, warns = collect_keys(res.cands, dump)
    prior, bad = load_prior(args.out)
    if bad:
        for (path, perrs) in bad:
            err("기존 번역 파일 파싱 오류(덮어쓰면 번역이 사라집니다): %s" % path)
            for (no, m) in perrs[:5]:
                err("  줄 %d: %s" % (no, m))
        if not args.force:
            err("중단: 오류를 고치거나 --force 로 덮어쓰십시오")
            return 1
    only = None
    if args.only:
        only = [x.strip().upper() for x in args.only.split(",") if x.strip()]
        for z in only:
            if z not in ZONES:
                err("알 수 없는 구역 %s (T1..T5)" % z)
                return 2
    os.makedirs(args.out, exist_ok=True)
    zones = [z for z in ZONES if only is None or z in only]
    for w in warns:
        err("경고: " + w)
    counts = {}
    for z in zones:
        if z == "T1" and dump is None:
            err("T1 건너뜀: --help-keys 가 없습니다")
            continue
        zk = [k for k in keys.values() if k.zone == z]
        if z == "T1":
            order = {e.en: i for i, e in enumerate(dump)}
            zk.sort(key=lambda k: order.get(k.en, 10 ** 9))
        else:
            zk.sort(key=lambda k: _own_first(k, z))
        entries = []
        filled = 0
        mism = 0
        last_file = None
        for k in zk:
            ko = ""
            p = prior.get(k.en)
            gok = ""
            if p:
                if p[0] == k.kind:
                    ko = p[1]
                    gok = p[2]
                    filled += 1
                else:
                    mism += 1
            comments = []
            if z == "T1":
                comments.extend(k.meta)
            else:
                if k.kind == "fmt" and k.vals:
                    comments.append("# 값: " + k.vals)
                ws = sorted(set(k.where), key=lambda w: (zone_of(w[0]) != z, _where_key(w)))
                locs = ", ".join(_fmt_where(w) for w in ws[:3])
                more = len(ws) - 3
                comments.append("# src: " + locs + (" 외 %d곳" % more if more > 0 else ""))
            if gok:
                comments.append("# glossary-ok: " + gok)   # 사람이 단 용어집 예외는 재생성해도 유지
            entries.append(Entry(k.kind, k.en, ko, comments))
        path = os.path.join(args.out, "ko.%s.toml" % z)
        stale = []
        if os.path.exists(path):
            old, _ = parse_table(read_text(path))
            have = {k.en for k in zk}
            stale = [e.en for e in old if e.ko != "" and e.en not in have and e.en not in keys]
        header = zone_header(z) + "# (이 파일의 en 은 고치지 않습니다. ko 만 채웁니다. 자세한 규칙은 ko.toml 머리 주석)\n"
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(format_table(entries, header))
        counts[z] = len(entries)
        err("%s: 항목 %d개 (기존 번역 채움 %d%s) -> %s" % (
            z, len(entries), filled, ", 종류 불일치로 못 채움 %d" % mism if mism else "", path))
        if stale:
            err("  주의: 이전 %s 에 있던 번역 %d개가 더는 소스에 없어 빠졌습니다(예: %s)" % (
                z, len(stale), one_line(stale[0], 60)))
    return 0


def zone_of_filename(path):
    k = part_key_of_filename(path)
    return k[0] if k else None


def cmd_merge(args):
    parts = []
    for f in args.files:
        pk = part_key_of_filename(f)
        if pk is None:
            err("구역 파일 이름이 아닙니다(ko.Tn.toml 또는 ko.Tn.pM.toml): %s" % f)
            return 2
        z = pk[0]
        entries, errors = parse_table(read_text(f))
        if errors:
            err("%s 파싱 오류:" % f)
            for (no, m) in errors[:10]:
                err("  줄 %d: %s" % (no, m))
            return 1
        parts.append((z, f, entries, pk[1]))
    parts.sort(key=lambda x: (x[0], x[3], x[1]))
    seen = {}
    out_chunks = []
    dup = 0
    for (z, f, entries, _pn) in parts:
        body = []
        for e in entries:
            if e.en in seen:
                dup += 1
                err("경고: 중복 키를 건너뜀(%s 가 %s 보다 앞 구역): %s" % (seen[e.en], z, one_line(e.en, 70)))
                continue
            seen[e.en] = z
            body.append(e)
        if out_chunks and out_chunks[-1][0] == z:
            out_chunks[-1][1].extend(body)   # 같은 구역의 조각은 구역 머리 주석 아래 이어 붙인다
        else:
            out_chunks.append((z, body))
    header = leading_header(args.out) or DEFAULT_HEADER
    texts = [header.rstrip("\n") + "\n"]
    total = 0
    for (z, body) in out_chunks:
        texts.append(zone_header(z))
        for e in body:
            texts.append(format_entry(e))
            total += 1
        # 구역 사이 빈 줄은 join 이 넣는다
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(texts))
    err("병합: %d개 파일, 항목 %d개, 중복 건너뜀 %d -> %s" % (len(parts), total, dup, args.out))
    return 0


def is_help_entry(e):
    """도움말 구역(T1) 항목인가: kind/path 주석이 있거나, 소스 출처 주석(# src:/# 값:)이 없는 항목."""
    if e.meta("kind") or e.meta("path"):
        return True
    return not (e.meta("src") or e.meta("값"))


def report(title, items, limit):
    if items:
        show_list(title, items, limit)


def cmd_check(args):
    table_path = args.table or KO_TOML
    if not os.path.exists(table_path):
        err("번역표가 없습니다: %s" % table_path)
        return 1
    entries, perrs = parse_table(read_text(table_path))
    res = check_entries(entries)
    allow = load_allow()
    src = scan_tree(allow=allow)
    dump = read_dump(args.help_keys) if args.help_keys else None
    keys, warns = collect_keys(src.cands, dump)
    by_en = {e.en: e for e in entries}
    missing = [en for en in keys if en not in by_en]
    kind_mismatch = ["%r: 소스는 [[%s]], 표는 [[%s]]" % (en, keys[en].kind, by_en[en].kind)
                     for en in keys if en in by_en and by_en[en].kind != keys[en].kind]
    stale = []
    pending = []
    for e in entries:
        if e.en in keys:
            continue
        # 덤프가 없으면 도움말 구역(T1)만 보류: 구역 머리·소스 형태로 비도움말 항목은 계속 낡음 판정
        if dump is None and is_help_entry(e):
            pending.append("줄 %d: %r" % (e.line, e.en))
        else:
            stale.append("줄 %d: %r" % (e.line, e.en))
    lim = args.limit
    print("번역표: %s — 항목 %d개 ([[text]] %d, [[fmt]] %d)" % (
        table_path, len(entries), sum(1 for e in entries if e.kind == "text"),
        sum(1 for e in entries if e.kind == "fmt")))
    n_dump = sum(1 for k in keys.values() if k.from_dump)
    synth_en = {r[0] for r in RUNTIME_KEYS}
    n_synth = sum(1 for en in keys if en in synth_en)
    print("키: 고유 %d개 (소스 %d, 덤프 %d, 합성 %d)" % (
        len(keys), len(keys) - n_dump - n_synth, n_dump, n_synth))
    hard = []
    hard += ["줄 %d: %s" % x for x in perrs]
    hard += res["errors"] + res["dups"] + kind_mismatch
    report("[오류] 파싱·서식·중복·종류", hard, lim)
    soft = {
        "열린 끝 위반(F15)": res["f15"],
        "문체 경고": res["style"],
        "용어집 경고": res["glossary"],
        "누락(표에 없음)": [repr(en) for en in missing],
        "미번역(ko=\"\")": res["untranslated"],
        "낡은 항목(소스·덤프에 없음)": stale,
    }
    for t, items in soft.items():
        report("[%s]" % ("엄격 실패" if args.strict else "경고") + " " + t, items, lim)
    report("[경고] [[text]] ko 의 {숫자}", res["warn"], lim)
    if pending:
        print("[보류] 소스에 없는 표 키 %d개 — --help-keys 덤프가 있어야 낡은 항목인지 판정합니다" % len(pending))
    for w in warns:
        err("경고: " + w)
    fail = bool(hard)
    if args.strict:
        fail = fail or any(soft[t] for t in soft)
    print("판정: %s%s" % ("실패" if fail else "통과", " (--strict)" if args.strict else ""))
    return 1 if fail else 0


def cmd_check_part(args):
    entries, perrs = parse_table(read_text(args.file))
    res = check_entries(entries)
    findings = ["줄 %d: %s" % x for x in perrs] + res["errors"] + res["dups"] + res["f15"] \
        + res["style"] + res["glossary"] + res["warn"]
    untr = res["untranslated"]
    print("%s: 항목 %d개" % (args.file, len(entries)))
    show_list("[문제]", findings, args.limit)
    show_list("[미번역 ko=\"\"]", untr, args.limit)
    bad = bool(findings) or bool(untr)
    print("판정: %s (문제 %d건, 미번역 %d건)" % ("실패" if bad else "통과", len(findings), len(untr)))
    return 1 if bad else 0


def build_parser():
    p = argparse.ArgumentParser(
        prog="extract.py",
        description="dx CLI 번역표(locales/ko.toml) 점검 도구. 저장소 어디서든 실행할 수 있습니다.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""사용 순서(번역 담당):
  1) 골격 만들기   extract.py skeleton --out DIR [--help-keys DUMP] [--only T2,T3,T4]
  2) 조각 채우기   DIR/ko.Tn.toml 의 ko 만 채운다 (en 은 고치지 않는다)
  3) 조각 점검     extract.py check-part DIR/ko.Tn.toml          (종료코드 0 이어야 함)
  4) 병합          extract.py merge DIR/ko.T*.toml --out packages/cli/locales/ko.toml
  5) 최종 점검     extract.py check --help-keys DUMP --strict
도움말 키 덤프: cargo test -p dioxus-cli --bin dx i18n::clap::tests::dump_help_keys -- --ignored --exact
                (환경변수 DX_I18N_DUMP=<경로> 로 저장 위치 지정)""")
    sub = p.add_subparsers(dest="cmd")
    a = sub.add_parser("scan", help="소스에서 뽑은 후보 목록 / 재현율 게이트")
    a.add_argument("--json", action="store_true", help="후보를 JSON 배열로 출력")
    a.add_argument("--unclassified", action="store_true",
                   help="추출·제외 범주·허용목록 어디에도 없는 문자열 리터럴만 출력(0줄이어야 함)")
    a.set_defaults(fn=cmd_scan)
    a = sub.add_parser("skeleton", help="구역별 번역 조각 ko.T1..T5.toml 골격 생성(기존 번역은 채움)")
    a.add_argument("--out", required=True, help="조각 파일을 쓸 디렉터리")
    a.add_argument("--help-keys", help="도움말 키 덤프 파일(없으면 T1 은 만들지 않음)")
    a.add_argument("--only", help="만들 구역만(예: T2,T3,T4). 나머지 파일은 건드리지 않음")
    a.add_argument("--force", action="store_true", help="기존 조각 파일에 파싱 오류가 있어도 덮어씀")
    a.set_defaults(fn=cmd_skeleton)
    a = sub.add_parser("merge", help="조각 파일들을 ko.toml 하나로 병합")
    a.add_argument("files", nargs="+", help="DIR/ko.T*.toml")
    a.add_argument("--out", required=True, help="결과 번역표 경로")
    a.set_defaults(fn=cmd_merge)
    a = sub.add_parser("check", help="번역표를 소스·덤프와 대조 검사")
    a.add_argument("--help-keys", help="도움말 키 덤프 파일")
    a.add_argument("--strict", action="store_true",
                   help="문체·용어집·열린 끝·누락·미번역·낡은 항목이 있어도 실패(종료코드 1)")
    a.add_argument("--table", help="검사할 번역표(기본: ko.toml)")
    a.add_argument("--limit", type=int, default=40, help="항목별 최대 출력 줄 수")
    a.set_defaults(fn=cmd_check)
    a = sub.add_parser("check-part", help="번역 조각 파일 1개 검사(문제 0·미번역 0 이어야 통과)")
    a.add_argument("file")
    a.add_argument("--limit", type=int, default=40)
    a.set_defaults(fn=cmd_check_part)
    a = sub.add_parser("selftest", help="내장 사례 검사")
    a.set_defaults(fn=lambda args: run_selftest())
    return p


def main(argv=None):
    p = build_parser()
    args = p.parse_args(argv)
    if not getattr(args, "fn", None):
        p.print_help()
        return 2
    try:
        return args.fn(args) or 0
    except BrokenPipeError:
        try:
            sys.stdout.close()
        except Exception:
            pass
        return 0


# ---------------------------------------------------------------------------
# 9. 자체 시험 (selftest)
# ---------------------------------------------------------------------------

def _extract(src, rel="build/x.rs"):
    an = Analysis(rel, src)
    cs, claimed = an.extract(collect_variants([an]))
    ms, _done = an.extract_modes(claimed)
    return cs + ms


def _keys(cs):
    return [(c.kind, c.en) for c in cs]


def _lex_str(src):
    toks = [t for t in lex(src) if t.k == "str"]
    return toks[0].v if toks else None


class _NS:
    def __init__(self, **kw):
        self.__dict__.update(kw)


def _ck(cond, msg=""):
    """python -O 에서도 동작하는 시험 단언."""
    if not cond:
        raise AssertionError(msg)


def _selftests():
    T = []

    def test(fn):
        T.append(fn)
        return fn

    @test
    def lexer_escapes():
        _ck(_lex_str(r'let a = "a\nb\t\"q\" \\ \u{AC00} \x41 \'";') == 'a\nb\t"q" \\ 가 A \'')

    @test
    def lexer_raw_strings():
        _ck(_lex_str('let a = r"x\\n y";') == "x\\n y")
        _ck(_lex_str('let a = r#"he said "hi" ok"#;') == 'he said "hi" ok')
        _ck(_lex_str('let a = r##"a"#b"##;') == 'a"#b')
        # 바이트 문자열은 str 이 아님
        _ck(_lex_str('let a = b"bytes here";') is None)

    @test
    def lexer_line_continuation():
        s = 'let a = "foo \\\n        bar \\\n    baz";'
        _ck(_lex_str(s) == "foo bar baz", _lex_str(s))

    @test
    def lexer_comments_chars_lifetimes():
        src = "// \"no\"\n/* \"no\" /* nested \"no\" */ */ fn f<'a>(x: &'a str) -> char { let c = '\"'; let d = '\\''; \"yes ok\" }"
        _ck([t.v for t in lex(src) if t.k == "str"] == ["yes ok"])

    @test
    def lexer_multiline_lines():
        toks = [t for t in lex('x(\n"a\nb\n\\\n c"\n)') if t.k == "str"]
        _ck((toks[0].v == "a\nb\nc" and toks[0].lines == [2, 3, 5][:len(toks[0].lines)] or toks[0].lines[0] == 2))

    @test
    def tracing_fields_skipped():
        cs = _extract('''fn f() {
            tracing::info!(dx_src = ?TraceSrc::Dev, target: "dx", telemetry = %json!({"a": "not msg"}),
                "Built {} files in {GLOW_STYLE}{}{GLOW_STYLE:#}", n, secs);
            warn!(?x, %y, "Careful now: {z}");
        }''')
        _ck(("fmt", "Built {} files in {GLOW_STYLE}{}{GLOW_STYLE:#}") in _keys(cs), _keys(cs))
        _ck(("fmt", "Careful now: {z}") in _keys(cs), _keys(cs))
        _ck(len(cs) == 2)

    @test
    def debug_trace_panic_expect_excluded():
        cs = _extract('''fn f() {
            tracing::debug!("debug message {}", 1);
            trace!("trace message {}", 2);
            panic!("panic message {}", 3);
            let _ = x.expect("expect message here");
            assert!(ok, "assert message here");
        }''')
        _ck(cs == [], _keys(cs))
        an = Analysis("build/x.rs", 'fn f() { debug!("debug message here {}", x); x.expect("expect text here"); }')
        why = [an.excluded_context(i) for i, t in enumerate(an.toks) if t.k == "str"]
        _ck(why == ["debug 전용", "패닉·단언"], why)

    @test
    def cfg_test_excluded():
        cs = _extract('''fn real() { bail!("real message {}", 1); }
        #[cfg(test)]
        mod tests {
            fn t() { bail!("test message {}", 2); }
        }
        #[test]
        fn solo() { bail!("solo test message {}", 3); }
        fn after() { bail!("after message {}", 4); }''')
        _ck([c.en for c in cs] == ["real message {}", "after message {}"], _keys(cs))

    @test
    def block_closure_tail():
        cs = _extract('''fn f() {
            a.with_context(|| { let x = 1; format!("Block closure {}", x) })?;
            b.with_context(move || {
                let p = path.display();
                format!("Move closure {p}")
            })?;
            c.with_context(
                || format!("Plain closure {}", 1),
            )?;
            d.context("Plain context message")?;
            e.with_context(|| "Literal tail text")?;
        }''')
        k = _keys(cs)
        for want in [("fmt", "Block closure {}"), ("fmt", "Move closure {p}"),
                     ("fmt", "Plain closure {}"), ("text", "Plain context message"),
                     ("text", "Literal tail text")]:
            _ck(want in k, (want, k))

    @test
    def multiline_closure_params_and_map_err_block():
        cs = _extract('''fn f() {
            x.map_err(|e| {
                tracing::debug!("ignored debug {}", e);
                anyhow::anyhow!("Mapped error: {}", e)
            })?;
            y.map_err(
                |e|
                {
                    format!("Second shape {}", e)
                }
            )?;
            z.ok_or_else(|| {
                if flag { format!("If branch {}", 1) } else { format!("Else branch {}", 2) }
            })?;
        }''')
        k = _keys(cs)
        for want in [("fmt", "Mapped error: {}"), ("fmt", "Second shape {}"),
                     ("fmt", "If branch {}"), ("fmt", "Else branch {}")]:
            _ck(want in k, (want, k))

    @test
    def variant_constructor():
        src = '''
        enum PatchError {
            #[error("Failed to read file: {0}")]
            Read(String),
            #[error("{0}")]
            InvalidModule(String),
        }
        fn f() -> Result<(), PatchError> {
            Err(PatchError::InvalidModule(format!("Bad section {}", name)))
        }
        fn g() -> Result<(), PatchError> {
            Err(PatchError::InvalidModule("Plain variant text".into()))
        }
        fn h() -> Result<(), PatchError> {
            Err(PatchError::Read(format!("Not human {}", 1)))
        }'''
        k = _keys(_extract(src))
        _ck(("fmt", "Bad section {}") in k, k)
        _ck(("text", "Plain variant text") in k, k)
        _ck(("fmt", "Failed to read file: {0}") in k, k)
        _ck(("fmt", "Not human {}") not in k, k)

    @test
    def status_native_plugin_head():
        k = _keys(_extract('''fn f(ctx: &Ctx) {
            ctx.status_compiling_native_plugins(format!("Kotlin build: {}", names.join(", ")));
        }'''))
        _ck(k == [("text", "Kotlin build")], k)

    @test
    def t_and_t_pad_keys_trimmed():
        k = _keys(_extract('''fn f() {
            let a = t_pad("App:    ");
            let b = crate::i18n::t("/:more");
            let c = tr_line(&format!("formatted {file}"));
        }''', rel="serve/x.rs"))
        _ck((("text", "App:") in k and ("text", "/:more") in k and ("fmt", "formatted {file}") in k), k)

    @test
    def raw_error_prefix():
        k = _keys(_extract('''fn f() {
            Err(clap::Error::raw(kind, format!("Unknown platform: {identifier}")));
            Err(clap::Error::raw(kind, "Desktop alias is not supported here"));
        }'''))
        _ck(("fmt", "error: Unknown platform: {identifier}") in k, k)
        _ck(("text", "error: Desktop alias is not supported here") in k, k)

    @test
    def multiline_fmt_split_by_line():
        cs = _extract('''fn f() {
            println!(
                r#"{LINK_STYLE}Setup{LINK_STYLE:#}
 {GLOW_STYLE}Web{GLOW_STYLE:#}: downloaded automatically

 Rustc version: {HINT_STYLE}{rustc_version}{HINT_STYLE:#}
 plain text line here
 iOS {x}
"#
            );
        }''', rel="cli/doctor.rs")
        d = {c.en: c for c in cs}
        _ck("{LINK_STYLE}Setup{LINK_STYLE:#}" in d)
        _ck("{GLOW_STYLE}Web{GLOW_STYLE:#}: downloaded automatically" in d)
        _ck("Rustc version: {HINT_STYLE}{rustc_version}{HINT_STYLE:#}" in d)
        _ck(d["plain text line here"].kind == "text")
        _ck("iOS {x}" not in d)  # 리터럴 알파벳 4 미만
        _ck(d["Rustc version: {HINT_STYLE}{rustc_version}{HINT_STYLE:#}"].line == 6, d["Rustc version: {HINT_STYLE}{rustc_version}{HINT_STYLE:#}"].line)

    @test
    def value_group_merge():
        en = "hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms"
        _ck(fmt_groups_count(en) == 1)
        _ck((fmt_groups_count("{} and {}: {} {name}") == 4))
        _ck(fmt_groups_count("{{literal}} braces {x}") == 1)
        cs = _extract('''fn f() { info!("hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms", elapsed); }''')
        _ck(cs[0].vals == "{1}={GLOW_STYLE}<elapsed>{GLOW_STYLE:#}", cs[0].vals)

    @test
    def brace_escapes_in_text():
        k = _keys(_extract('''fn f() { bail!("Use {{curly}} braces here"); }'''))
        _ck(k == [("text", "Use {curly} braces here")], k)

    @test
    def f15_open_end_violation():
        e = Entry("fmt", "Failed to read {}", "{1}을(를) 읽지 못했습니다", line=1)
        errs, f15 = fmt_errors(e)
        _ck((not errs and f15), (errs, f15))
        ok = Entry("fmt", "Failed to read {}", "읽지 못했습니다: {1}", line=1)
        _ck(fmt_errors(ok) == ([], []))
        start = Entry("fmt", "{} is not supported", "지원하지 않습니다: {1}", line=1)
        _ck(fmt_errors(start)[1])
        start_ok = Entry("fmt", "{} is not supported", "{1}은(는) 지원하지 않습니다", line=1)
        _ck(fmt_errors(start_ok) == ([], []))

    @test
    def open_end_needs_eight_literal_letters():
        _ck(fmt_is_open("from {libssl_source:?}") and fmt_is_open("{} changed"))
        _ck(not fmt_is_open("{LINK_STYLE}Setup{LINK_STYLE:#}"))
        _ck(fmt_is_open("{ERROR_STYLE}Build failed{ERROR_STYLE:#}: {}"))
        _ck(not fmt_is_open("took {} ms") and not fmt_is_open("a {HINT_STYLE}({x}){HINT_STYLE:#}"))
        _ck(fmt_is_open("in {GLOW_STYLE}{}{GLOW_STYLE:#}"))
        bad = Entry("fmt", "from {libssl_source:?}", "출처 {1}", line=1)
        _ck(any("열린 끝" in m for m in fmt_errors(bad)[0]), fmt_errors(bad))
        okk = Entry("fmt", "Loaded from {src}", "출처: {1}", line=1)
        _ck(fmt_errors(okk) == ([], []), fmt_errors(okk))
        closed = Entry("fmt", "[{}] done [{}]", "[{1}] 끝 [{2}]", line=1)
        _ck(fmt_errors(closed) == ([], []), fmt_errors(closed))
        k = _keys(_extract('''fn f() {
            let a = tr_line(&format!("from {libssl_source:?}"));
            let b = tr_line(&format!("Loaded from {src}"));
            let c = tr_line(&format!("{LINK_STYLE}Setup{LINK_STYLE:#}"));
        }''', rel="serve/x.rs"))
        _ck(("fmt", "from {libssl_source:?}") not in k, k)
        _ck(("fmt", "Loaded from {src}") in k, k)

    @test
    def read_dump_missing_file_exits_2():
        try:
            read_dump("/nonexistent/help-keys.toml")
        except SystemExit as ex:
            _ck(ex.code == 2, ex.code)
        else:
            _ck(False, "SystemExit 없음")

    @test
    def short_t_keys_are_candidates():
        k = _keys(_extract('''fn f() {
            let a = t("r: x");
            let b = t_pad("OK ");
        }''', rel="serve/x.rs"))
        _ck(("text", "r: x") in k and ("text", "OK") in k, k)

    @test
    def f24_every_group_used_at_least_once():
        e = Entry("fmt", "{} and {}: done now", "{1} 끝", line=1)
        errs, _ = fmt_errors(e)
        _ck(any("사용하지 않은" in m for m in errs), errs)
        rep = Entry("fmt", "{} and {}: done now", "{1}{2}{1} 끝", line=1)
        _ck(not fmt_errors(rep)[0])
        oob = Entry("fmt", "value {} here", "{2} 값", line=1)
        _ck(any("범위 밖" in m for m in fmt_errors(oob)[0]))

    @test
    def fmt_rejects_too_few_literal_letters():
        _ck(fmt_errors(Entry("fmt", "{}: {}", "{1}: {2}", line=1))[0])
        _ck(fmt_errors(Entry("fmt", "no placeholders at all", "x", line=1))[0])

    @test
    def style_checked_before_open_end_placeholder():
        e = Entry("fmt", "Failed to read file: {}", "파일을 읽지 못했어요: {1}")
        _ck(style_warnings(e), "해요체 + 끝 값 묶음을 놓침")
        e = Entry("fmt", "Failed to read file: {}", "파일을 읽지 못했다: {1}")
        _ck(style_warnings(e), "해라체 + 끝 값 묶음을 놓침")
        e = Entry("fmt", "Failed to read file: {}", "파일을 읽지 못했습니다: {1}")
        _ck(not style_warnings(e), style_warnings(e))
        e = Entry("fmt", "Failed to read file ({})", "파일을 읽지 못했어요 ({1})")
        _ck(style_warnings(e))

    @test
    def glossary_scoped_terms():
        # target 은 Rust target triple 문맥에서만 강제
        _ck(not glossary_warnings(Entry("text", "Target a web app", "웹 앱 대상", line=1)))
        _ck(glossary_warnings(Entry("text", "Unknown cargo target triple", "알 수 없는 대상", line=1)))

    @test
    def runtime_keys_injected():
        keys, _w = collect_keys([])
        _ck(("Caused by:" in keys and keys["Caused by:"].zone == "T5" and keys["Caused by:"].kind == "text"))

    @test
    def allow_regex_does_not_drop_candidates():
        res = ScanResult()
        al = parse_allow("re:</[a-z]+> ## 템플릿: x\nx.rs:Debug only ## debug 전용: y\n")
        _ck([e.kind for e in al] == ["re", "prefix"])
        _ck(al[0].matches("any.rs", "see </dict> here"))
        # scan_tree 는 prefix 항목만 후보에서 뺀다 — 구현 계약 확인
        import inspect
        _ck('e.kind == "prefix"' in inspect.getsource(scan_tree))

    @test
    def stale_help_entry_detection():
        h = Entry("text", "x y", "", ["# kind: help", "# path: dx"])
        src = Entry("text", "x y", "", ["# src: a.rs:1"])
        _ck((is_help_entry(h) and not is_help_entry(src)))

    @test
    def duplicate_keys_detected():
        entries, errors = parse_table('[[text]]\nen = "Same key"\nko = "a"\n[[fmt]]\nen = "Same key"\nko = "b"\n')
        _ck(not errors)
        _ck(check_entries(entries)["dups"])

    @test
    def toml_roundtrip():
        original = [
            Entry("text", 'Quote " and \\ back\ttab', "따옴표 \" 와 \\ 와\t탭", ["# kind: about", "# path: dx"]),
            Entry("fmt", "Failed to read {}", "읽지 못했습니다: {1}", ["# 값: {1}=<path>", "# src: a.rs:1"]),
            Entry("text", "line\nbreak", "줄\n바꿈 \u001b[0m 제어"),
            Entry("text", "unicode ✓ 한글", ""),
        ]
        text = format_table(original, "# header comment\n")
        entries, errors = parse_table(text)
        _ck(not errors, errors)
        _ck([(e.kind, e.en, e.ko) for e in entries] == [(e.kind, e.en, e.ko) for e in original])
        _ck((entries[0].meta("kind") == "about" and entries[0].meta("path") == "dx"))
        _ck(entries[1].meta("값").startswith("{1}="))

    @test
    def toml_rejects_forbidden_forms():
        for bad in ['[[text]]\nen = """x"""\nko = "y"\n',
                    "[[text]]\nen = 'x'\nko = \"y\"\n",
                    '[[text]]\nen = "x"\nko = "y"\nextra = "z"\n',
                    '[[other]]\nen = "x"\nko = "y"\n',
                    '[[text]]\nen = "x\\q"\nko = "y"\n',
                    '[[text]]\nen = "x"\n']:
            _entries, errors = parse_table(bad)
            _ck(errors, bad)

    @test
    def style_and_glossary_checks():
        bad = Entry("text", "Hot reload is on.", "핫 리로드가 켜졌어요.", line=1)
        _ck(style_warnings(bad), "해요체 미검출")
        _ck(style_warnings(Entry("text", "Done now", "끝났다", line=1)), "해라체 미검출")
        _ck(not style_warnings(Entry("text", "Done now", "끝났습니다", line=1)))
        _ck(not style_warnings(Entry("text", "Needed now", "필요", line=1)), "필요 는 명사")
        _ck(style_warnings(Entry("text", "Done now.", "끝났습니다", line=1)), "마침표 불일치 미검출")
        _ck(glossary_warnings(Entry("text", "Bundle the asset", "묶습니다", line=1)))
        _ck(not glossary_warnings(Entry("text", "Bundle the asset", "에셋을 번들로 묶습니다", line=1)))
        _ck(not glossary_warnings(Entry("text", "Run `dx bundle` now", "지금 실행합니다", line=1)), "백틱 예외")
        _ck(not glossary_warnings(Entry("text", "Pass --target x", "인자를 전달합니다", line=1)), "옵션 예외")
        _ck(glossary_warnings(Entry("text", "Missing directory", "디렉토리가 없습니다", line=1)))

    @test
    def dump_format_read():
        dump = '''# kind: about
# path: dx
[[text]]
en = "Build the project"
ko = ""

# kind: clap_error
# path: -
[[fmt]]
en = "error: unexpected argument '{}' found"
ko = ""
'''
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "dump.toml")
            with open(p, "w", encoding="utf-8") as fh:
                fh.write(dump)
            es = read_dump(p)
            _ck(([(e.kind, e.meta("kind"), e.meta("path")) for e in es] == [
                ("text", "about", "dx"), ("fmt", "clap_error", "-")]))
            keys, _w = collect_keys([], es)
            _ck((keys["Build the project"].zone == "T1" and keys["Build the project"].from_dump))

    @test
    def allowlist_forms():
        al = parse_allow("""# c
re:^\\s*<[A-Za-z] ## 템플릿: 태그
a/b.rs:re:^x+$ ## 파일 정규식
a/b.rs:Prefix text ## 파일 앞부분
c.rs ## 파일 전체
""")
        _ck([e.kind for e in al] == ["re", "re", "prefix", "file"])
        _ck(al[0].matches("any.rs", "  <div>"))
        _ck((al[1].matches("a/b.rs", "xxx") and not al[1].matches("z.rs", "xxx")))
        _ck((al[2].matches("a/b.rs", "  Prefix text and more") and not al[2].matches("a/b.rs", "no")))
        _ck(al[3].matches("c.rs", "whatever"))
        _ck(al[0].category == "템플릿")

    @test
    def gate_classification():
        _ck((qualifies("two words here") and not qualifies("oneword") and not qualifies("a b")))
        an = Analysis("build/x.rs", '''fn f() {
            bail!("extracted message {}", 1);
            debug!("debug only message");
            let _ = "some leaked message";
            if s.contains("pattern text here") {}
            Command::new("tool").arg("--flag value here");
        }''')
        cs, claimed = an.extract(collect_variants([an]))
        left = []
        for i, t in enumerate(an.toks):
            if t.k == "str" and i not in claimed and qualifies(t.v) and not an.excluded_context(i):
                left.append(t.v)
        _ck(left == ["some leaked message"], left)

    @test
    def part_check_exit_codes():
        import tempfile
        good = '''# == 구역: T2 ==
# src: a.rs:1
[[text]]
en = "Failed to find package"
ko = "패키지를 찾지 못했습니다"

# 값: {1}=<p>
[[fmt]]
en = "Failed to read {}"
ko = "읽지 못했습니다: {1}"
'''
        badfile = good + '''
[[fmt]]
en = "Open ended {}"
ko = "{1} 로 시작"

[[text]]
en = "Needs work"
ko = ""
'''
        with tempfile.TemporaryDirectory() as d:
            g = os.path.join(d, "ko.T9.toml")
            b = os.path.join(d, "ko.T8.toml")
            with open(g, "w", encoding="utf-8") as fh:
                fh.write(good)
            with open(b, "w", encoding="utf-8") as fh:
                fh.write(badfile)
            import io
            import contextlib
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                _ck(cmd_check_part(_NS(file=g, limit=5)) == 0)
                _ck(cmd_check_part(_NS(file=b, limit=5)) == 1)

    @test
    def skeleton_is_non_destructive():
        import tempfile
        import io
        import contextlib
        res = scan_tree(allow=load_allow())
        keys, _w = collect_keys(res.cands)
        t2 = [k for k in keys.values() if k.zone == "T2" and k.kind == "text"]
        _ck(t2, "T2 text 키가 없습니다")
        target = t2[0]
        with tempfile.TemporaryDirectory() as d:
            old = Entry("text", target.en, "미리 번역된 값", [])
            with open(os.path.join(d, "ko.T2.toml"), "w", encoding="utf-8") as fh:
                fh.write(format_table([old], "# old\n"))
            sentinel = os.path.join(d, "ko.T3.toml")
            with open(sentinel, "w", encoding="utf-8") as fh:
                fh.write("# 건드리면 안 됨\n")
            err_buf = io.StringIO()
            with contextlib.redirect_stderr(err_buf):
                rc = cmd_skeleton(_NS(out=d, help_keys=None, only="T2"))
            _ck(rc == 0)
            _ck(read_text(sentinel) == "# 건드리면 안 됨\n", "--only 밖 파일이 바뀜")
            _ck(not os.path.exists(os.path.join(d, "ko.T1.toml")), "--help-keys 없이 T1 생성")
            entries, errors = parse_table(read_text(os.path.join(d, "ko.T2.toml")))
            _ck(not errors, errors)
            got = {e.en: e for e in entries}
            _ck(got[target.en].ko == "미리 번역된 값", "기존 번역을 채우지 못함")
            fmts = [e for e in entries if e.kind == "fmt"]
            _ck((fmts and all(e.meta("값") or e.meta("src") for e in fmts)))
            _ck(all(e.meta("src") for e in entries))

    @test
    def merge_orders_and_dedups():
        import tempfile
        import io
        import contextlib
        with tempfile.TemporaryDirectory() as d:
            t3 = os.path.join(d, "ko.T3.toml")
            t2 = os.path.join(d, "ko.T2.toml")
            with open(t3, "w", encoding="utf-8") as fh:
                fh.write('[[text]]\nen = "Shared key"\nko = "삼"\n\n[[text]]\nen = "Only three"\nko = "삼만"\n')
            with open(t2, "w", encoding="utf-8") as fh:
                fh.write('[[text]]\nen = "Shared key"\nko = "이"\n')
            out = os.path.join(d, "ko.toml")
            with contextlib.redirect_stderr(io.StringIO()):
                _ck(cmd_merge(_NS(files=[t3, t2], out=out)) == 0)
            entries, errors = parse_table(read_text(out))
            _ck(not errors)
            _ck([(e.en, e.ko) for e in entries] == [("Shared key", "이"), ("Only three", "삼만")], entries)

    @test
    def doctor_after_wrapping_shape():
        cs = _extract('''fn f() {
            let a = "not found".to_string();
            println!("{}", crate::i18n::tr_text(&format!(r#"{LINK_STYLE}Setup{LINK_STYLE:#}
 Rustc path: {HINT_STYLE}{rustc_path}{HINT_STYLE:#}
"#)));
            let b = crate::i18n::t("not installed");
        }''', rel="cli/doctor.rs")
        k = _keys(cs)
        _ck(("fmt", "{LINK_STYLE}Setup{LINK_STYLE:#}") in k, k)
        _ck(("fmt", "Rustc path: {HINT_STYLE}{rustc_path}{HINT_STYLE:#}") in k, k)
        _ck((("text", "not found") in k and ("text", "not installed") in k), k)

    @test
    def conversions_err_body_field_const_and_branches():
        cs = _extract('''
        const NOTIFY_ERROR_MSG: &str = "Failed to create watcher now";
        const OTHER_URL: &str = "https://example.com/some thing here";
        fn f() {
            x.context("Needs conversion here".to_string())?;
            Err("Plain err text here".into());
            Err(format!("Formatted err {}", 1));
            let _ = Body::from("Body message text here");
            let s = Status::Building { progress: 0.0, build_message: "Starting build now...".to_string() };
            let o = Outcome::Ignore { note: Some(format!("Note about {}", name)) };
            a.with_context(|| {
                if x { format!("First branch {}", 1) } else if y { "Second branch text".to_string() } else { format!("Last branch {}", 2) }
            })?;
            b.map_err(|e| match e { Kind::A => anyhow!("Arm A failed {}", 1), Kind::B => { format!("Arm B failed {}", 2) } })?;
            let z = Struct { description: "Not a human field here" };
        }''')
        k = _keys(cs)
        for want in [("text", "Failed to create watcher now"), ("text", "Needs conversion here"),
                     ("text", "Plain err text here"), ("fmt", "Formatted err {}"),
                     ("text", "Body message text here"), ("text", "Starting build now..."),
                     ("fmt", "Note about {}"), ("fmt", "First branch {}"), ("text", "Second branch text"),
                     ("fmt", "Last branch {}"), ("fmt", "Arm A failed {}"), ("fmt", "Arm B failed {}")]:
            _ck(want in k, (want, k))
        _ck(("text", "https://example.com/some thing here") not in k)
        _ck(("text", "Not a human field here") not in k)

    @test
    def extra_site_and_all_mode_function():
        cs = _extract('''fn hotreload_mode_label() -> &'static str {
            match m { A => "hot-patching", B => "rsx and assets", C => "disabled" }
        }
        fn other() { let x = "should not appear"; }''', rel="serve/runner.rs")
        k = _keys(cs)
        _ck((k == [("text", "hot-patching"), ("text", "rsx and assets"), ("text", "disabled")]), k)
        cs = _extract('''fn f() { let e = if a { format!("\\n  Learn more at {LINK_STYLE}https://x.y/{LINK_STYLE:#}") } else { String::new() }; }''', rel="serve/mod.rs")
        _ck(("fmt", "Learn more at {LINK_STYLE}https://x.y/{LINK_STYLE:#}") in _keys(cs), _keys(cs))

    @test
    def i18n_dir_and_test_harness_skipped():
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, "i18n"))
            for name, body in [("i18n/mod.rs", 'fn f() { bail!("In i18n dir {}", 1); }'),
                               ("test_harnesses.rs", 'fn f() { bail!("In harness {}", 1); }'),
                               ("real.rs", 'fn f() { bail!("In real file {}", 1); }')]:
                with open(os.path.join(d, name), "w", encoding="utf-8") as fh:
                    fh.write(body)
            names = [rel for rel, _p in source_files(d)]
            _ck(names == ["real.rs"], names)


    @test
    def part_file_names_and_merge_of_parts():
        _ck(part_key_of_filename("x/ko.T2.p1.toml") == ("T2", 1))
        _ck(part_key_of_filename("ko.T3.toml") == ("T3", 0))
        _ck((part_key_of_filename("ko.T2.px.toml") is None and zone_of_filename("ko.T2.p10.toml") == "T2"))
        import tempfile
        import io
        import contextlib
        def tbl(en, ko):
            return '# == 구역 ==\n# src: a.rs:1\n[[text]]\nen = "%s"\nko = "%s"\n' % (en, ko)
        with tempfile.TemporaryDirectory() as d:
            f2 = os.path.join(d, "ko.T2.p2.toml")
            f1 = os.path.join(d, "ko.T2.p1.toml")
            f3 = os.path.join(d, "ko.T3.toml")
            for (f, en, ko) in ((f2, "Second part", "둘째"), (f1, "First part", "첫째"), (f3, "Third zone", "셋째")):
                with open(f, "w", encoding="utf-8") as fh:
                    fh.write(tbl(en, ko))
            out = os.path.join(d, "merged.toml")
            with contextlib.redirect_stderr(io.StringIO()):
                _ck(cmd_merge(_NS(files=[f3, f2, f1], out=out)) == 0)
            text = read_text(out)
            _ck(text.count("# == 구역: T2") == 1 and text.count("# == 구역: T3") == 1, text)
            _ck(text.index("First part") < text.index("Second part") < text.index("Third zone"))
            prior, bad = load_prior(d)
            _ck(not bad and prior["First part"][1] == "첫째" and prior["Second part"][1] == "둘째", prior)

    @test
    def hyphen_terms_are_checked():
        _ck(glossary_warnings(Entry("text", "Failed to hot-reload the app", "앱을 다시 불러오지 못했습니다", line=1)))
        _ck(not glossary_warnings(Entry("text", "Failed to hot-reload the app", "앱을 핫 리로드하지 못했습니다", line=1)))
        _ck(glossary_warnings(Entry("text", "hot-patching failed", "다시 적용하지 못했습니다", line=1)))
        _ck("hot-reload" in clean_for_glossary("Failed to hot-reload the app"))
        _ck("--hot-reload" not in clean_for_glossary("Pass --hot-reload now"))

    @test
    def glossary_context_exclusion_and_comment():
        en = "Invalid version component in version string"
        _ck(not glossary_warnings(Entry("text", en, "버전 구성 요소가 올바르지 않습니다", line=1)))
        _ck(glossary_warnings(Entry("text", "Invalid component name", "이름이 올바르지 않습니다", line=1)))
        _ck(not glossary_warnings(Entry("text", "Global assets must have at least one file component",
                                        "에셋에는 파일 이름 구성 요소가 하나 이상 있어야 합니다", line=1)))
        e = Entry("text", "Unknown device class", "알 수 없는 종류입니다", ["# glossary-ok: device"], line=1)
        _ck(not glossary_warnings(e))
        e.comments = []
        _ck(glossary_warnings(e))

    @test
    def style_hae_yo_variants_and_banned_release():
        for ko in ("잘 돼요", "이걸 봐요", "열어 줘요", "할게요", "괜찮대요", "아파워요", "여기 와요."):
            _ck(style_warnings(Entry("text", "Done now", ko, line=1)), ko)
        for ko in ("필요", "중요", "개요", "요약"):
            _ck(not [w for w in style_warnings(Entry("text", "Done now", ko, line=1)) if "해요체" in w], ko)
        _ck(glossary_warnings(Entry("text", "New release", "새 릴리즈", line=1)))
        _ck(not glossary_warnings(Entry("text", "New release", "새 릴리스", line=1)))

    @test
    def line_continuation_keeps_source_line():
        src = 'fn f() {\n    println!("Line one here\\n\\\n        line two here");\n}'
        cs = _extract(src, rel="cli/doctor.rs")
        got = {c.en: c.line for c in cs}
        _ck(got.get("Line one here") == 2 and got.get("line two here") == 3, got)

    @test
    def t_with_surrounding_space_is_reported():
        an = Analysis("serve/x.rs", 'fn f() { let a = t("App: "); let b = t_pad("Ok: "); }')
        an.extract(collect_variants([an]))
        _ck(len(an.errors) == 1 and "t_pad" in an.errors[0][2], an.errors)

    return T


def run_selftest():
    tests = _selftests()
    failed = 0
    for fn in tests:
        try:
            fn()
            print("ok   %s" % fn.__name__)
        except Exception as e:  # noqa: BLE001 — 시험 실패 보고용
            failed += 1
            import traceback
            print("FAIL %s: %s" % (fn.__name__, e))
            traceback.print_exc(limit=3)
    print("selftest: %d개 중 %d개 실패" % (len(tests), failed))
    return 1 if failed else 0


if __name__ == "__main__":
    for _s in (sys.stdout, sys.stderr):
        try:
            _s.reconfigure(encoding="utf-8")
        except Exception:
            pass
    sys.exit(main())
