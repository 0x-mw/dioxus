# dx CLI 한국어 포크 안내서

이 문서는 `0x-mw/dioxus` 포크(브랜치 `ko-kr`)를 유지보수하는 개발자를 위한 안내서입니다.
번역표는 `packages/cli/locales/ko.toml`, 점검 도구는 `packages/cli/locales/extract.py` 입니다.
아래 명령은 별도 표시가 없으면 저장소 루트에서 실행합니다.

## 1. 개요

이 포크는 `dx`(Dioxus CLI)의 화면 출력을 한국어로 보여 줍니다. 도움말, 로그, 오류 메시지, `dx serve` 화면의 고정 문구, `dx doctor` 등이 대상입니다.

동작 원리는 다음과 같습니다. 소스 코드 속 영어 문구는 고치지 않고 그대로 둡니다. 문구가 화면에 나가기 직전에 번역표 `packages/cli/locales/ko.toml`(바이너리에 내장)과 맞춰 보고, 일치하는 항목이 있으면 한국어로 바꿔 출력합니다. 번역표에 없는 문구는 영어 그대로 나옵니다. 그래서 upstream 이 새 문구를 추가해도 동작은 깨지지 않고 영어로 폴백합니다.

## 2. 언어 전환

기본은 한국어입니다. 환경 변수 `DX_LANG` 이 `en` 으로 시작하면(대소문자 무관) 번역표를 쓰지 않고 upstream 과 같은 영어 출력을 냅니다.

```bash
DX_LANG=en dx serve --platform web     # 이번 실행만 영어
export DX_LANG=en                      # 이 셸에서는 계속 영어
unset DX_LANG                          # 다시 한국어
```

## 3. 번역되지 않는 것

다음은 의도적으로 영어(원문)로 둡니다.

- JSON 출력(`--json-output`)과 로그 파일(`--log-to-file`), 텔레메트리: 기계가 읽는 출력입니다.
- `dx print`, `dx completions`, `dx translate`, `dx fmt` 가 만들어 내는 코드·스크립트·스키마 출력.
- `dx --version` 출력(스크립트가 파싱합니다).
- rustc·cargo 의 컴파일러 메시지, 사용자 앱이 내는 출력, 패닉 메시지(버그 보고용).
- debug·trace 수준 로그.
- `dx new` 의 템플릿 질문: 외부 저장소(dioxus-template, cargo-generate)가 내는 문구입니다.
- clap 이 만드는 내장 문구 중 일부, 특히 줄바꿈으로 잘려 한 줄에 다 들어오지 않는 도움말 문구.
- 번역표에 아직 없는 새 upstream 문구(영어로 폴백).

## 4. 번역표 고치기

`ko.toml` 은 `[[text]]` 와 `[[fmt]]` 두 종류의 표만 씁니다. 필드는 `en`, `ko` 둘뿐입니다. 파일 머리 주석에 같은 규칙이 요약되어 있습니다.

```toml
[[text]]                      # 완전 일치: 도움말·고정 문구
en = "Build the app"
ko = "앱을 빌드합니다"

[[fmt]]                       # 서식 패턴: 값이 끼는 문구
en = "Failed to read {} from {}"
ko = "{2}에서 {1}을(를) 읽지 못했습니다"
```

규칙은 다음과 같습니다.

- `[[text]]` 의 `en` 은 화면에 나오는 문자열과 완전히 같아야 하고, `ko` 에는 `{n}` 을 쓰지 않습니다.
- `[[fmt]]` 의 `en` 은 Rust 서식 문자열 한 줄이며, 자리표시자가 1개 이상이고 리터럴 알파벳이 4자 이상이어야 합니다.
- 값 묶음: 리터럴 없이 붙은 자리표시자(`{GLOW_STYLE}{}{GLOW_STYLE:#}`)는 값 묶음 1개입니다. 번호는 왼쪽부터 `{1}`, `{2}`, ... 입니다. `ko` 에서는 번호로 참조하며, 모든 묶음을 최소 1회 써야 하고 순서는 바꿔도 되고 같은 번호를 반복해도 됩니다. 문자 그대로의 중괄호는 `{{`, `}}` 입니다.
- 열린 끝 규칙: `en` 이 값 묶음으로 끝나면 `ko` 도 마지막 묶음 참조로 끝나야 하고, 값 묶음으로 시작하면 `ko` 도 `{1}` 로 시작해야 합니다. 예) `Failed to read {}` 는 `읽지 못했습니다: {1}` (O), `{1}을(를) 읽지 못했습니다` (X).
- 한 줄 문자열 규칙: 값은 `"..."` 한 줄만 씁니다. 줄바꿈은 `\n`, 그 밖에 `\" \\ \t \uXXXX` 만 허용하고 여러 줄 문자열(`"""`)과 리터럴 문자열(`'...'`)은 금지입니다.
- 같은 `en` 이 두 번 나오면(종류 무관) 오류입니다.
- `ko = en`(항등)은 번역이 필요 없는 문구로 처리를 마쳤다는 표시이고, `ko = ""` 는 아직 번역하지 않았다는 표시(건너뜀)입니다.

문체와 용어(`docs/task-id/ko-i18n/glossary.md` 요약):

- 합니다체로 통일합니다. `Failed to X` 는 "X하지 못했습니다", `X not found` 는 "X을(를) 찾을 수 없습니다", 진행 표시는 "빌드하는 중..." 처럼 씁니다.
- 명령·옵션 이름, 경로, 코드, 플랫폼 이름(web, desktop, iOS, Android 등)은 영어로 둡니다.
- 문장 끝 마침표, 앞뒤 공백, 들여쓰기는 원문 그대로 둡니다. TUI 고정 라벨은 한글 1자가 2칸이므로 표시 폭을 맞춥니다.
- 값 뒤 조사는 문장을 바꿔 피하고("파일을 읽지 못했습니다: {1}"), 불가피하면 "을(를)"로 병기합니다.
- 핵심 용어: hot-reload 핫 리로드, hot-patch 핫 패치, bundle 번들, asset 에셋, target 타깃, crate 크레이트, feature 피처, workspace 워크스페이스, telemetry 텔레메트리.

번역표를 고친 뒤에는 `ko.toml` 이 바이너리에 내장되므로 반드시 다시 설치해야 반영됩니다(6절).

```bash
python3 packages/cli/locales/extract.py check          # 빠른 점검
cargo install --path packages/cli --locked --force     # 다시 설치
```

## 5. 점검 도구 extract.py

Python 3.10 이상, 표준 라이브러리만 씁니다. 저장소 어디서 실행해도 됩니다. 자세한 옵션은 `python3 packages/cli/locales/extract.py <하위명령> --help` 로 확인합니다.

| 하위 명령 | 용도 |
|---|---|
| `scan` | 소스에서 뽑은 번역 후보 목록, 재현율 게이트 |
| `skeleton` | 구역별 번역 조각 `ko.T1..T5.toml` 골격 생성(기존 번역은 채움) |
| `merge` | 조각 파일들을 `ko.toml` 하나로 병합 |
| `check` | 번역표를 소스·덤프와 대조 검사 |
| `check-part` | 번역 조각 파일 1개 검사(문제 0·미번역 0 이어야 통과) |
| `selftest` | 내장 사례로 추출기 자체를 검사 |

```bash
# 후보 목록 / 재현율 게이트
python3 packages/cli/locales/extract.py scan --json                # 후보를 JSON 배열로
python3 packages/cli/locales/extract.py scan --unclassified        # 어디에도 분류되지 않은 리터럴(0줄이어야 함)

# 번역표 검사 (--strict 는 문체·용어집·열린 끝·누락·미번역·낡은 항목이 있어도 실패, 종료코드 1)
python3 packages/cli/locales/extract.py check
python3 packages/cli/locales/extract.py check --help-keys /tmp/help-keys.txt --strict

# 조각 작업 흐름 (새 문구가 많을 때)
python3 packages/cli/locales/extract.py skeleton --out /tmp/parts --help-keys /tmp/help-keys.txt --only T2,T3,T4
python3 packages/cli/locales/extract.py check-part /tmp/parts/ko.T2.toml
python3 packages/cli/locales/extract.py merge /tmp/parts/ko.T*.toml --out packages/cli/locales/ko.toml

# 추출기 자체 점검
python3 packages/cli/locales/extract.py selftest
```

조각 파일에서는 `ko` 만 채우고 `en` 은 고치지 않습니다.

도움말(clap) 문구의 키는 실행 시 명령 트리를 훑어야 알 수 있으므로, 테스트로 덤프 파일을 만듭니다. `DX_I18N_DUMP` 로 저장 위치를 지정합니다.

```bash
DX_I18N_DUMP=/tmp/help-keys.txt cargo test -p dioxus-cli --bin dx \
  i18n::clap::tests::dump_help_keys -- --ignored --exact
```

빌드 산출물을 C: 드라이브(Windows 파일시스템)에 쌓지 않으려면 `CARGO_TARGET_DIR=/home/mw/.cache/dioxus-target` 를 앞에 붙입니다.

허용목록 `packages/cli/locales/extract_allow.txt` 는 `scan --unclassified` 게이트에서 "화면 메시지가 아닌 것"을 걸러 내는 목록입니다. HTML·JS·CSS 템플릿, 셸·cargo 인자, 경로, 생성 파일 내용, 텔레메트리·내부 식별자, debug 전용 문자열, 기계 출력만 담습니다. 항목마다 끝에 ` ## 범주: 사유` 가 필수입니다. 원칙은 사람에게 보이는 메시지는 숨기지 않는다는 것입니다. 새 문구가 걸리면 허용목록에 넣지 말고 번역표에 추가하거나 `extract.py` 의 추출 규칙을 고칩니다.

## 6. 설치·업데이트

Rust(rustup)가 필요합니다. CLI 의 `rust-version` 은 1.93.0 이고, 웹 플랫폼을 쓰려면 `wasm32-unknown-unknown` 타깃도 필요합니다.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

설치와 재설치는 같은 명령입니다.

```bash
cargo install --path packages/cli --locked --force
```

`pkg-config` 가 없는 WSL 에서는 openssl-sys 가 실패하므로 환경 변수를 붙입니다.

```bash
OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu OPENSSL_INCLUDE_DIR=/usr/include \
  cargo install --path packages/cli --locked --force
```

설치 위치는 `~/.cargo/bin/dx` 입니다. 이미 열려 있는 터미널에서는 `source ~/.cargo/env` 를 실행하거나 새 셸을 엽니다.

```bash
source ~/.cargo/env
command -v dx            # ~/.cargo/bin/dx 여야 합니다
dx --version             # 예: dioxus 0.8.0-alpha.1 (1a2b3c4) — 괄호 안은 빌드한 커밋 해시 7자리
git rev-parse --short=7 ko-kr
```

`dx --version` 의 괄호 안 해시가 `git rev-parse --short=7 ko-kr` 결과와 같으면 현재 `ko-kr` 커밋으로 빌드된 것입니다. 다르면 오래된 빌드이므로 다시 설치합니다.

## 7. 경고: `dx self-update` 를 실행하지 마십시오

`dx self-update` 는 한국어 포크 바이너리를 upstream 의 영어 릴리스로 덮어씁니다. 업데이트가 필요하면 8절의 upstream 동기화를 한 뒤 `cargo install --path packages/cli --locked --force` 로 다시 설치합니다. `dx` 가 새 버전을 알릴 때 upstream 원문은 `dx self-update` 를 권하지만, 이 포크의 번역표는 그 알림을 "이 한국어 포크에서는 dx self-update 대신 저장소를 갱신한 뒤 cargo install --path packages/cli 로 다시 설치하십시오"로 옮겨 둡니다(`DX_LANG=en` 으로 실행하면 원문이 나오므로 그때도 따르지 않습니다).

## 8. upstream 동기화 (merge 방식)

`ko-kr` 에 `main` 을 merge 하는 방식이라 강제 푸시가 필요 없습니다. `main` 은 upstream 과 같게만 유지하고 직접 커밋하지 않습니다.

```bash
git fetch upstream
git checkout main && git merge --ff-only upstream/main && git push origin main
git checkout ko-kr && git merge main
```

충돌이 나면 주로 아래 파일입니다. 이 포크가 고친 곳은 출력 문자열을 번역 함수로 감싼 부분과 진입점 교체뿐이므로, 충돌 시 upstream 쪽 변경을 받아들이고 번역 함수 호출만 다시 감싸면 됩니다.

- `packages/cli/src/logging.rs`
- `packages/cli/src/cli/platform_override.rs`
- `packages/cli/src/serve/output.rs`
- `packages/cli/src/cli/run.rs`
- `packages/cli/src/cli/doctor.rs`
- `packages/cli/src/workspace.rs`
- `packages/cli/src/cli/create.rs`
- `packages/cli/src/check/issues.rs`
- `packages/cli/src/cli/component.rs`
- `packages/cli/src/cli/autoformat.rs`
- `packages/cli/src/cli/bundle.rs`
- `packages/cli/Cargo.toml`

merge 가 끝나면 새로 생기거나 바뀐 문구를 확인하고 번역을 보충합니다.

```bash
DX_I18N_DUMP=/tmp/help-keys.txt cargo test -p dioxus-cli --bin dx \
  i18n::clap::tests::dump_help_keys -- --ignored --exact
python3 packages/cli/locales/extract.py check --help-keys /tmp/help-keys.txt --strict
python3 packages/cli/locales/extract.py scan --unclassified
```

`check` 가 보고하는 누락·낡은 항목을 `ko.toml` 에 반영하고(4절), 테스트를 통과시킨 뒤 다시 설치합니다.

```bash
cargo test -p dioxus-cli
cargo install --path packages/cli --locked --force
git push origin ko-kr
```

## 9. 되돌리기

- 일시적으로 영어로: `DX_LANG=en dx …` 또는 `export DX_LANG=en`. upstream 과 같은 경로로 동작합니다.
- 번역표만 비우기: `packages/cli/locales/ko.toml` 의 `[[text]]`·`[[fmt]]` 항목을 지우고(머리 주석은 남김) 다시 설치하면 출력이 전부 영어로 돌아갑니다. 번역 커밋을 `git revert` 해도 됩니다.
- 설치 되돌리기: 포크 바이너리를 지우거나 upstream 을 설치합니다.

```bash
cargo uninstall dioxus-cli
cargo install dioxus-cli --locked      # upstream 릴리스 설치(영어)
```
