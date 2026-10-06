<p>
    <p align="center" >
      <!-- <img src="../../../notes/header-light-updated.svg#gh-light-mode-only" >
      <img src="../../../notes/header-dark-updated.svg#gh-dark-mode-only" > -->
      <!-- <a href="https://dioxuslabs.com">
          <img src="../../../notes/flat-splash.avif">
      </a> -->
      <img src="../../../notes/splash-header-darkmode.svg#gh-dark-mode-only" style="width: 80%; height: auto;">
      <img src="../../../notes/splash-header.svg#gh-light-mode-only" style="width: 80%; height: auto;">
      <!-- <img src="../../../notes/image-splash.avif"> -->
      <br>
    </p>
</p>
<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/dioxus">
    <img src="https://img.shields.io/crates/v/dioxus.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/dioxus">
    <img src="https://img.shields.io/crates/d/dioxus.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs -->
  <a href="https://docs.rs/dioxus">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
  <!-- CI -->
  <a href="https://github.com/jkelleyrtp/dioxus/actions">
    <img src="https://github.com/dioxuslabs/dioxus/actions/workflows/main.yml/badge.svg"
      alt="CI status" />
  </a>

  <!--Awesome -->
  <a href="https://dioxuslabs.com/awesome">
    <img src="https://cdn.rawgit.com/sindresorhus/awesome/d7305f38d29fed78fa85652e3a63e154dd8e8829/media/badge.svg" alt="Awesome Page" />
  </a>
  <!-- Discord -->
  <a href="https://discord.gg/XgGxMSkvUM">
    <img src="https://img.shields.io/discord/899851952891002890.svg?logo=discord&style=flat-square" alt="Discord Link" />
  </a>
</div>

<div align="center">
  <h3>
    <a href="https://dioxuslabs.com"> 웹사이트 </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/dioxus/tree/main/examples"> 예제 </a>
    <span> | </span>
    <a href="https://dioxuslabs.com/learn/0.7/tutorial"> 튜토리얼 </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/dioxus/blob/main/notes/translations/zh-cn/README.md"> 中文 </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/dioxus/blob/main/notes/translations/pt-br/README.md"> PT-BR </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/dioxus/blob/main/notes/translations/ja-jp/README.md"> 日本語 </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/dioxus/blob/main/notes/translations/tr-tr"> Türkçe </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/dioxus/blob/main/notes/translations/ko-kr"> 한국어 </a>
  </h3>
</div>
<br>
<!-- <p align="center">
  <a href="https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.0">✨ Dioxus 0.7 is out!!! ✨</a>
</p> -->
<br>

하나의 코드베이스로 웹, 데스크톱, 모바일 등 여러 플랫폼용 앱을 빌드합니다. 설정이 필요 없는 환경, 통합 핫 리로드, 시그널 기반 상태 관리를 제공합니다. 서버 함수로 백엔드 기능을 추가하고 CLI로 번들을 만들 수 있습니다.

```rust
fn app() -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        h1 { "High-Five counter: {count}" }
        button { onclick: move |_| count += 1, "Up high!" }
        button { onclick: move |_| count -= 1, "Down low!" }
    }
}
```

## ⭐️ 고유 기능:

- 세 줄의 코드로 만드는 크로스 플랫폼 앱 (웹, 데스크톱, 모바일, 서버 등)
- React, Solid, Svelte의 장점을 결합한 [사용하기 편한 상태 관리](https://dioxuslabs.com/blog/release-050)
- 기능이 풍부하고 타입 안전한 풀스택 웹 프레임워크 내장
- 웹, macOS, Linux, Windows에 배포할 수 있는 통합 번들러
- 1초 이내에 적용되는 Rust 핫 패치와 에셋 핫 리로드
- 그 밖에도 많습니다! [Dioxus 둘러보기](https://dioxuslabs.com/learn/0.7/).

## 즉각적인 핫 리로드

`dx serve` 명령 하나면 앱이 실행됩니다. 마크업과 스타일을 수정하면 몇 밀리초 안에 변경 사항을 확인할 수 있습니다. 실험적 기능인 `dx serve --hotpatch`를 사용하면 Rust 코드도 실시간으로 업데이트할 수 있습니다.

<div align="center">
  <img src="https://raw.githubusercontent.com/DioxusLabs/screenshots/refs/heads/main/blitz/hotreload-video.webp">
  <!-- <video src="https://private-user-images.githubusercontent.com/10237910/386919031-6da371d5-3340-46da-84ff-628216851ba6.mov" width="500"></video> -->
  <!-- <video src="https://private-user-images.githubusercontent.com/10237910/386919031-6da371d5-3340-46da-84ff-628216851ba6.mov" width="500"></video> -->
</div>

## 아름다운 앱 만들기

Dioxus 앱은 HTML과 CSS로 스타일을 지정합니다. 내장된 TailwindCSS 지원을 사용하거나 즐겨 쓰는 CSS 라이브러리를 불러올 수 있습니다. 네이티브 코드(objective-c, JNI, Web-Sys)를 쉽게 호출해 완벽한 네이티브 느낌을 낼 수 있습니다.

<div align="center">
  <img src="../../../notes/ebou2.avif">
</div>



## 진정한 풀스택 앱

Dioxus는 [axum](https://github.com/tokio-rs/axum)과 깊이 통합되어 클라이언트와 서버 모두에 강력한 풀스택 기능을 제공합니다. WebSockets, SSE, 스트리밍, 파일 업로드/다운로드, 서버 사이드 렌더링, 폼, 미들웨어, 핫 리로드 등 다양한 내장 기능 중에서 골라 쓰거나, 완전히 직접 구성하여 기존 axum 백엔드와 통합할 수 있습니다.

<div align="center">
  <img src="../../../notes/fullstack-websockets.avif" width="700">
</div>

## 실험적 네이티브 렌더러

web-sys, webview, 서버 사이드 렌더링, liveview로 렌더링하거나 실험적 WGPU 기반 렌더러를 쓸 수 있습니다. Bevy, WGPU에 Dioxus를 내장하거나 임베디드 Linux에서 실행할 수도 있습니다!

<div align="center">
  <img src="https://raw.githubusercontent.com/DioxusLabs/screenshots/refs/heads/main/blitz/native-blitz-wgpu.webp">
</div>


## 공식 기본 컴포넌트

shadcn/ui와 Radix-Primitives를 본떠 만든 완전한 기본 컴포넌트 세트로 빠르게 시작하십시오.

<div align="center">
  <img src="https://raw.githubusercontent.com/DioxusLabs/screenshots/refs/heads/main/blitz/dioxus-components.webp" width="700">
</div>

## Android와 iOS 완벽 지원

Dioxus는 Rust로 네이티브 모바일 앱을 만드는 가장 빠른 방법입니다. `dx serve --platform android`를 실행하면 몇 초 만에 에뮬레이터나 기기에서 앱이 실행됩니다. JNI와 네이티브 API를 직접 호출할 수 있습니다.

<div align="center">
  <img src="../../../notes/android_and_ios2.avif" width="500">
</div>



## 웹, 데스크톱, 모바일용 번들

`dx bundle`을 실행하면 앱이 빌드되고 최대한 최적화된 번들로 묶입니다. 웹에서는 [`.avif` 생성, `.wasm` 압축, 최소화](https://dioxuslabs.com/learn/0.7/tutorial/assets) 등을 활용할 수 있습니다. [50kb 미만](https://github.com/ealmloff/tiny-dioxus/)의 웹 앱과 5mb 미만의 데스크톱/모바일 앱을 빌드할 수 있습니다.

<div align="center">
  <img src="../../../notes/bundle.gif">
</div>


## 훌륭한 문서

깔끔하고 읽기 쉬우며 포괄적인 문서를 만드는 데 많은 노력을 기울였습니다. 모든 html 요소와 리스너에는 MDN 문서가 달려 있으며, 문서 사이트는 Dioxus 본체와 함께 지속적 통합(CI)을 돌려 늘 최신 상태를 유지합니다. 가이드, 레퍼런스, 레시피 등은 [Dioxus 웹사이트](https://dioxuslabs.com/learn/0.7/)에서 확인하십시오. 재미있는 사실: Dioxus 웹사이트는 새로운 Dioxus 기능을 시험하는 테스트베드로 쓰입니다 - [살펴보십시오!](https://github.com/dioxusLabs/docsite)

<div align="center">
  <img src="../../../notes/docs.avif">
</div>


## 커뮤니티

Dioxus는 커뮤니티가 이끄는 프로젝트이며 [Discord](https://discord.gg/XgGxMSkvUM)와 [GitHub](https://github.com/DioxusLabs/dioxus/issues) 커뮤니티가 매우 활발합니다. 저희는 언제나 도움의 손길을 환영하며, 질문에 답하고 처음 시작하는 분을 기꺼이 돕습니다. [저희 SDK](https://github.com/DioxusLabs/dioxus-std)는 커뮤니티가 운영하며, 최고의 Dioxus 크레이트에 무료 업그레이드와 지원을 제공하는 [GitHub 조직](https://github.com/dioxus-community/)도 있습니다.

<div align="center">
  <img src="../../../notes/dioxus-community.avif">
</div>

## 풀타임 코어 팀

Dioxus는 사이드 프로젝트에서 풀타임 엔지니어로 이루어진 작은 팀으로 성장했습니다. FutureWei, Satellite.im, GitHub Accelerator 프로그램의 넉넉한 후원 덕분에 Dioxus 개발에 풀타임으로 전념할 수 있습니다. 장기 목표는 고품질 유료 엔터프라이즈 도구를 제공하여 Dioxus가 스스로 지속 가능해지는 것입니다. 귀사가 Dioxus 도입에 관심이 있고 저희와 함께 일하고 싶다면 연락해 주십시오!

## 지원 플랫폼

<div align="center">
  <table style="width:100%">
    <tr>
      <td>
      <b>웹</b>
      </td>
      <td>
        <ul>
          <li>WebAssembly를 사용하여 DOM에 직접 렌더링</li>
          <li>SSR로 사전 렌더링하고 클라이언트에서 하이드레이션</li>
          <li>React와 비슷한 약 50kb 크기의 간단한 "hello world"</li>
          <li>빠르게 고쳐 보며 개발할 수 있는 내장 개발 서버와 핫 리로드</li>
        </ul>
      </td>
    </tr>
    <tr>
      <td>
      <b>데스크톱</b>
      </td>
      <td>
        <ul>
          <li>Webview로 렌더링하거나 실험적으로 WGPU 또는 <a href="https://freyaui.dev">Freya</a> (Skia)로 렌더링 </li>
          <li>별도 설정 불필요. `cargo run` 또는 `dx serve`만으로 앱 빌드 </li>
          <li>IPC 없이 네이티브 시스템에 접근하는 기능을 완벽하게 지원 </li>
          <li>macOS, Linux, Windows 지원. 3mb 미만의 이식성 높은 바이너리 </li>
        </ul>
      </td>
    </tr>
    <tr>
      <td>
      <b>모바일</b>
      </td>
      <td>
        <ul>
          <li>Webview로 렌더링하거나 실험적으로 WGPU 또는 Skia로 렌더링 </li>
          <li>iOS와 Android용 .ipa, .apk 파일 빌드 </li>
          <li>최소한의 오버헤드로 Java와 Objective-C를 직접 호출</li>
          <li>"hello world"부터 기기 실행까지 단 몇 초</li>
        </ul>
      </td>
    </tr>
    <tr>
      <td>
      <b>서버 사이드 렌더링</b>
      </td>
      <td>
        <ul>
          <li>서스펜스, 하이드레이션, 서버 사이드 렌더링</li>
          <li>서버 함수로 백엔드 기능을 빠르게 추가</li>
          <li>익스트랙터, 미들웨어, 라우팅 통합</li>
          <li>정적 사이트 생성과 증분 재생성</li>
        </ul>
      </td>
    </tr>
  </table>
</div>

## 예제 실행하기

> 이 저장소 main 브랜치의 예제는 git 버전의 dioxus와 CLI를 대상으로 합니다. 최신 안정 릴리스의 dioxus에서 동작하는 예제를 찾고 있다면 [0.6 브랜치](https://github.com/DioxusLabs/dioxus/tree/v0.6/examples)를 확인하십시오.

이 저장소 최상위의 예제는 다음 명령으로 실행할 수 있습니다:

```sh
cargo run --example <example>
```

그러나 핫 리로드 같은 기능을 사용해 보려면 dioxus-cli를 다운로드하는 것을 권장합니다. 최신 CLI 바이너리는 cargo binstall로 설치할 수 있습니다.

```sh
curl -fsSL https://dioxuslabs.com/install.sh | bash
```

이 CLI가 최신이 아니라면 git 또는 cargo-binstall로 직접 설치할 수 있습니다:

```sh
cargo install --git https://github.com/DioxusLabs/dioxus dioxus-cli --locked
```

CLI를 사용하면 web 플랫폼에서도 예제를 실행할 수 있습니다. 다음 명령으로 기본 desktop 피처를 끄고 web 피처를 켜야 합니다:

```sh
dx serve --example <example> --platform web -- --no-default-features
```

## 기여하기

- 웹사이트의 [기여 안내 섹션](https://dioxuslabs.com/learn/0.7/beyond/contributing)을 확인하십시오.
- [이슈 트래커](https://github.com/dioxuslabs/dioxus/issues)에 문제를 제보하십시오.
- Discord에 [참여](https://discord.gg/XgGxMSkvUM)하여 질문하십시오!

<a href="https://github.com/dioxuslabs/dioxus/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=dioxuslabs/dioxus&max=30&columns=10" />
</a>

## 라이선스

이 프로젝트에는 [MIT 라이선스] 또는 [Apache-2 라이선스] 중 하나가 적용됩니다.

[apache-2 라이선스]: https://github.com/DioxusLabs/dioxus/blob/master/LICENSE-APACHE
[mit 라이선스]: https://github.com/DioxusLabs/dioxus/blob/master/LICENSE-MIT

별도로 명시하지 않는 한, 귀하가 Dioxus에 포함하려는 의도로 제출한 모든 기여에는 추가 조건 없이 MIT 또는 Apache-2 라이선스가 적용됩니다.
