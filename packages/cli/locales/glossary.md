# dx CLI 한국어 번역 지침·용어집

`ko.toml` 의 번역문을 쓰거나 고칠 때 따르는 문체·용어 규칙이다. 번역표 형식 규칙은 `ko.toml` 머리 주석과 포크 안내서(`README.md`) 4절을 따르며, 둘이 충돌하면 그쪽이 우선한다. `extract.py check --strict` 가 문체·용어 일부를 자동으로 검사한다.

## 1. 문체

- **합니다체로 통일**: 도움말·로그·오류 모두. 예) "앱을 빌드합니다.", "파일을 읽지 못했습니다", "설정 파일이 없습니다".
  - `Failed to X` → "X하지 못했습니다" (명사형이 자연스러우면 "X에 실패했습니다")
  - `Could not find X` / `X not found` → "X을(를) 찾을 수 없습니다"
  - `Unsupported X` → "지원하지 않는 X입니다"
  - `Invalid X` → "올바르지 않은 X입니다"
  - 진행형 로그(`Building...`, `Downloading X`) → "빌드하는 중...", "X 다운로드 중" (짧은 진행 표시는 "~ 중" 허용)
  - 질문/확인 → "~하시겠습니까?"
- **TUI 상태 라벨**(serve 화면의 짧은 고정 문구)은 명사형 허용: "컴파일 중", "링크 중", "번들링 중", "대기 중".
- **문장 끝 마침표는 원문을 따른다**: 원문이 `.`로 끝나면 번역도 `.`, 아니면 없음.
- **앞뒤 공백·줄바꿈·들여쓰기는 원문 그대로** 보존한다 (TUI 정렬·도움말 형식이 깨진다).
- **TUI 고정 라벨은 표시 폭을 맞춘다**: 터미널에서 한글 1자 = 2칸. 원문이 `"App:    "`처럼 공백으로 폭을 맞춘 라벨이면, 번역도 같은 표시 폭이 되도록 뒤 공백을 조정한다. (예: `"App:    "`(8칸) → `"앱:     "`(앱=2칸 + `:` 1칸 + 공백 5칸 = 8칸))

## 2. 그대로 두는 것 (번역 금지)

- 명령·하위명령·옵션·플래그 이름: `dx serve`, `--release`, `--platform web`, `-v`
- 파일·경로·확장자·설정 키: `Cargo.toml`, `Dioxus.toml`, `[web.app]`, `.dioxus/`, `ANDROID_HOME`
- 코드·식별자·백틱(`` ` ``) 안의 내용, URL
- 플랫폼·제품·도구 이름: web, desktop, mobile, iOS, Android, macOS, Windows, Linux, liveview, server, WASM/wasm, wasm-bindgen, wasm-opt, esbuild, TailwindCSS, NSIS, WiX, Gradle, Xcode, xcrun, codesign, linuxdeploy, AppImage, rustc, cargo, rustup, RSX, Dioxus, GitHub, VSCode, Cursor
  - 예외(README 리뷰에서 확정): web/desktop/mobile/server 가 **식별자 문맥**(`--platform` 값, cargo 피처 이름, 설정 키)이면 영어 그대로("web 플랫폼", "desktop 피처"), **일반 범주를 뜻하는 산문**이면 한국어("웹, 데스크톱, 모바일용 앱")로 옮긴다. CLI 도움말·메시지에서 플랫폼을 고르는 문맥은 대부분 식별자 문맥이다.
- 자리표시자 묶음: 번역문의 `{1}`, `{2}`, … 는 원문 값 묶음의 순서다. **모든 묶음을 최소 한 번씩** 쓴다(빠뜨리지 않는다. 같은 묶음 반복은 허용). 값 순서는 한국어 어순에 맞게 바꿔도 된다.

- **열린 끝 규칙(필수, 검사기가 강제)**: 원문이 값 묶음으로 **끝나면** 번역문도 그 마지막 묶음 `{N}` 으로 끝나야 하고, 원문이 값 묶음으로 **시작하면** 번역문도 `{1}` 로 시작해야 한다. 예) `Failed to read {}` → `읽지 못했습니다: {1}` (O) / `{1}을(를) 읽지 못했습니다` (X). `{} is not supported` → `{1}은(는) 지원하지 않습니다` (O). 값 안에 예상 못 한 영어가 섞여도 한국어 문장이 깨지지 않게 하려는 규칙이다.

## 3. 조사 처리

값(`{N}`) 바로 뒤의 조사는 값의 받침을 알 수 없으므로:
1. 먼저 문장을 바꿔 피한다: 값 뒤에 명사를 붙이거나("{1} 파일을 읽지 못했습니다"), 값을 콜론 뒤로 보낸다("파일을 읽지 못했습니다: {1}").
2. 피할 수 없으면 병기한다: 을(를), 이(가), 은(는), 과(와), (으)로, 이라는/라는 → "(이)라는".

## 4. 용어집

| 영어 | 한국어 | 비고 |
|---|---|---|
| hot-reload / hot reloading | 핫 리로드 | 합의 |
| hot-patch / hotpatch | 핫 패치 | 합의 |
| bundle (n.) / bundling | 번들 / 번들링 | 합의. 동사 bundle → "번들로 묶다", "번들을 만들다" |
| asset | 에셋 | 합의 |
| target (Rust target triple) | 타깃 | 합의 |
| crate | 크레이트 | 합의 |
| feature (cargo) | 피처 | 합의. feature flag → 피처 플래그 |
| workspace | 워크스페이스 | 합의 |
| telemetry | 텔레메트리 | 합의 |
| app / application | 앱 | |
| project | 프로젝트 | |
| package (cargo) | 패키지 | |
| dependency | 의존성 | |
| platform | 플랫폼 | |
| renderer | 렌더러 | |
| build (n./v.) | 빌드 / 빌드하다 | rebuild → 다시 빌드 |
| compile | 컴파일 | |
| link / linker | 링크 / 링커 | |
| binary / executable | 바이너리 / 실행 파일 | fat binary → fat 바이너리 |
| library | 라이브러리 | |
| symbol / segment | 심볼 / 세그먼트 | |
| artifact(s) | 빌드 산출물 | |
| archive | 아카이브 | |
| output | 출력 (파일이면 결과물) | |
| directory / folder | 디렉터리 / 폴더 | "디렉토리" 아님 |
| path | 경로 | |
| config / configuration / settings | 설정 | config file → 설정 파일 |
| manifest | 매니페스트 | |
| template | 템플릿 | |
| component | 컴포넌트 | |
| registry | 레지스트리 | |
| dev server | 개발 서버 | serve (v.) → "개발 서버로 띄우다" / 상태 "실행 중" |
| server / client | 서버 / 클라이언트 | |
| fullstack | 풀스택 | |
| server function | 서버 함수 | |
| proxy | 프록시 | |
| address / port | 주소 / 포트 | |
| route | 라우트 | |
| static site generation (SSG) | 정적 사이트 생성(SSG) | |
| pre-render | 사전 렌더링 | |
| hydration | 하이드레이션 | |
| device | 기기 | |
| simulator / emulator | 시뮬레이터 / 에뮬레이터 | |
| code signing / codesigning | 코드 서명 | |
| notarization | 공증 | |
| provisioning profile | 프로비저닝 프로파일 | |
| entitlements | 엔타이틀먼트 | |
| certificate | 인증서 | |
| keystore | 키스토어 | |
| installer | 설치 프로그램 | |
| download / install / update | 다운로드 / 설치 / 업데이트 | |
| toolchain | 툴체인 | |
| environment variable | 환경 변수 | |
| log / verbose / trace | 로그 / 자세한 로그 / 추적 로그 | |
| interactive (mode) | 대화형 (모드) | |
| watch / watcher | 감시 / 파일 감시기 | |
| restart | 재시작 | |
| release / debug (mode) | 릴리스 / 디버그 (모드) | |
| profile (cargo) | 프로필 | |
| cache / temporary | 캐시 / 임시 | |
| offline | 오프라인 | |
| deprecated | 더 이상 지원되지 않는 | |
| experimental | 실험적 | |
| default | 기본값 | "[default: x]"는 clap 표기 |
| argument / option / flag | 인자 / 옵션 / 플래그 | |
| command / subcommand | 명령 / 하위 명령 | |
| shell completions | 셸 자동 완성 | |
| format / autoformat (RSX) | 포맷 / 자동 포맷 | |
| translate (`dx translate`, HTML→RSX) | 변환 | |
| check (`dx check`) | 검사 | |
| doctor (`dx doctor`) | 진단 | |
| self-update | 자체 업데이트 | |
| issue (check result) | 문제 | |
| warning / error | 경고 / 오류 | |
