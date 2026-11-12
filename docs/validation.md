# Dropper 검증 기록

2026-11-12 · macOS Apple Silicon 환경. 제품에 가짜 압축 성공이나 고정 감소율을 넣지 않았다.

## 실행한 검사

| 명령·검사 | 결과 |
|---|---|
| `cargo test --no-default-features` (src-tauri에서 실행, v0.2.0 lock 갱신 포함) | Rust 33개 통과 |
| `cargo clippy --all-targets --no-default-features -- -D warnings` (src-tauri에서 실행) | 통과 |
| `cargo clippy --all-targets -- -D warnings` (src-tauri에서 실행) | Tauri 데스크톱 연결까지 통과 |
| `npm test` | UI 모델 6개 통과 |
| `npm run build` | TypeScript 검사·Vite 빌드 통과 |
| `npm run test:ui` + 설치된 Chromium 경로 지정 | Playwright 6개 통과 |
| `python3 -m unittest discover -s tests/packaging -v` | 다운로드 중단·불완전 파일 거부·정상 캐시·부분 파일 교체 4개 통과 |
| `cargo check` | macOS Tauri 연결 컴파일 통과 |
| `npm run tauri build -- --bundles app,dmg` | macOS 앱·DMG 생성 |
| `cargo xwin build --release --locked --target x86_64-pc-windows-msvc --target-dir target-windows --xwin-sdk-version 10.0.22621` (src-tauri에서 실행) | Windows x64 실행 파일 생성 |
| `makensis scripts/windows-installer.nsi` | Windows 설치 파일 생성 |
| `7zz t artifacts/Dropper_0.2.0_x64-setup.exe` | 13개 파일 무결성 통과 |
| `sh scripts/build-mac-intel.sh` | Intel macOS 앱·DMG 생성, 실제 실행은 하지 않음 |

UI 검사는 920×780, 720×560, 390×844에서 가로 잘림이 없고 주요 동작에 스크롤로 접근 가능한지 확인했다. 브라우저 미리보기가 데스크톱 압축을 흉내 내지 않는지, 품질 단일 선택과 접힌 상세 설정도 검사했다.

Rust 검사는 절감량·성장·0바이트 계산, 표본 구간, 추정, 스트림 형식 정책, HDR 제한, 셸 없는 인자, 실제 H.264/H.265/AV1, HDR10 정적 메타데이터/10비트 보존, 다중 오디오·언어·자막, 회전 방향, VFR 타임스탬프, 큰 결과 미저장, 이름 충돌, 심볼릭 링크 출력 거부, 취소 전/실행 중 종료, 변경된 입력, 등록되지 않은 ID, 작업 중복 방지, 재추가 갱신을 포함한다.

## 실제 앱 조작

생성한 `Dropper.app`의 기본 파일 선택창에서 직접 만든 `드롭퍼-검증영상.mov`를 선택했다. 균형 모드의 H.265 시험 압축은 **약 573 KB / 예상 88.5% 감소**, 전체 압축 결과는 **562 KB / 실제 88.7% 감소**로 표시됐다. 원본은 **4.97 MB**였다. 실제 완료 화면의 용량·절감량과 폴더 열기 버튼을 확인했다. 이 수치는 이 검증용 ProRes 영상에만 해당한다.

## 독립 리뷰와 수정

별도의 읽기 전용 리뷰에서 최소 macOS 버전과 동봉 엔진 요구 버전 불일치, 예상 작업이 완료 상태를 초기화하는 문제, 원본 재추가 시 오래된 캐시 유지, mov_text의 MKV 강제 복사 문제를 찾았다. 최소 macOS 버전은 실제 엔진에 맞춰 26.0으로 조정했다. 나머지는 회귀 테스트를 작성해 실패를 확인하고 수정한 뒤 전체 검사를 실행했다.

새 Windows 패키징 코드도 별도로 읽기 전용 리뷰를 받았다. WebView2 다운로드 중단 시 부분 파일을 재사용하는 결함을 재현했다. 임시 파일 다운로드·HTTP Content-Length 확인·완료 후 원자적 교체와 해시/크기 캐시 검증으로 수정하고 4개 회귀 테스트를 통과했다. 앱과 FFmpeg/ffprobe의 Windows PE import 목록은 시스템 DLL만 참조하며 별도 누락된 코덱 DLL을 발견하지 못했다.

최종 macOS Info.plist의 최소 버전은 26.0이며, 번들 안의 FFmpeg는 `/usr/bin:/bin`만 PATH에 둔 실행에서도 정상 동작했다.

Intel Mac용 앱과 FFmpeg/ffprobe는 모두 x86_64 Mach-O임을 확인했다. Intel 정적 엔진은 macOS 10.13 이상, 앱은 12.0 이상을 대상으로 한다. Intel 앱 리소스에는 `media` 하나만 있고 Apple Silicon 엔진·라이브러리가 섞이지 않았다. 이 호스트에는 Rosetta가 없어 Intel 실행 검증은 하지 않았다. Windows 설치 파일에서 추출한 앱과 WebView2 설치 도우미의 SHA-256이 최종 입력 파일과 일치함도 확인했다.

검증 중 Rust 릴리스 proc-macro dylib의 LINKEDIT 정렬 문제를 재현했다. [Rust 이슈 #157750](https://github.com/rust-lang/rust/issues/157750)의 조건에 해당해 release 프로필의 strip을 `none`으로 두었으며 이후 릴리스 빌드가 통과했다.

## v0.2.0 폴더 기능 검증

`folder.rs`의 별도 탐색·출력 정책과 `folder_import.rs`의 실제 FFmpeg 일괄 작업 검사를 추가했다. 하위 폴더, 대문자 확장자, 영상 외 파일, 기존 결과 폴더 제외, 601개 파일 탐색, 사용자 지정 결과 제외, 취소, 순환 링크, 상대 경로 탈출 거부, 출력 경로의 링크 거부, 파일·폴더 혼합 입력, 사용자 지정 저장 위치, 탐색·분석 취소와 작업 잠금 해제, 601개 작업 인자의 중복 제거와 실제 파일을 지우지 않는 목록 비우기를 검사했다.

폴더 기능의 독립 리뷰에서 혼합 입력 순서에 따른 경로 유실, 원본 부모를 저장 위치로 골랐을 때 하위 영상 누락, 직접 드롭한 링크를 따라가는 문제를 발견했다. 세 가지 모두 실패 테스트를 확인한 뒤 수정했다. 폴더 등록 정보가 개별 파일보다 우선하며, 원본과 겹치는 출력 루트는 `_Compressed`로 분리하고 링크 입력을 보고 후 제외한다.

새 macOS 앱(v0.2.0)의 **폴더 추가**에서 검증용 `dropper-folder-check`를 선택했다. `2024/same.mov`, `2025/same.mov` 두 파일(총 9.93 MB)을 자동 수집하고 기존 `Compressed` 폴더 하나를 제외했다. **2개 영상 압축**으로 모두 처리한 뒤 실제 절감량 **8.81 MB / 88.7%**를 표시했다. 결과가 각각 `Compressed/2024/same_compressed.mp4`, `Compressed/2025/same_compressed.mp4`에 생겼으며 원본 SHA-256, 기존 결과 파일, 영상 외 텍스트 파일이 유지됐다. 이 감소율은 해당 테스트 영상의 결과이다.

이번 버전의 자동 검사 합계는 **49개: Rust 33, UI 상태 6, UI 화면 6, 패키징 4**이다. 플랫폼별 설치 파일은 v0.2.0으로 다시 생성했으며 Windows·Intel의 실제 실행은 여전히 미검증이다.

## 남은 실행 검증 범위

- Windows 설치·실행·WebView2 설치 흐름: Windows 머신/VM이 없어 미실행. PE 파일과 NSIS 생성은 런타임 검증을 대신하지 않는다.
- macOS Intel 실제 실행: Intel 머신 또는 Rosetta 실행 환경이 없어 미실행. 전용 앱·엔진·DMG는 생성했다.
- HLG의 실제 디스플레이 색 재현, 모든 HDR/코덱/해상도 조합, 대용량 장시간 파일: 전수 검증 아님.
- 코드 인증서 서명·Apple 공증·스토어 배포: 수행하지 않음.

Apple Silicon 번들의 FFmpeg와 dylib 요구 버전은 26.0이며 이 빌드에 대해 macOS 12–15 지원이라고 주장하지 않는다. Intel 별도 빌드는 macOS 12를 대상으로 만들었지만 실제 실행 확인과 구분한다. 모든 확장자 내 모든 코덱 지원이나 보장된 압축률을 주장하지 않는다.
