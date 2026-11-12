# Dropper
영상 용량 떨구기

**드롭퍼 — 영상의 무게를 덜어내다.**

파일을 넣고 압축 강도를 고르면, 내 컴퓨터에서 영상을 압축하는 개인용 데스크톱 앱입니다. 서버 업로드나 계정이 없으며 원본을 덮어쓰지 않습니다.

## 사용하기

1. 영상을 창에 끌어놓거나 **파일 선택하기**를 누릅니다. 여러 파일을 한 번에 추가할 수 있습니다.
2. **화질 우선 / 균형 / 용량 우선**을 고릅니다. 기본은 균형입니다.
3. 필요하면 **예상 용량 확인**을 눌러 짧은 시험 압축으로 용량을 추정합니다.
4. **영상 압축**을 누릅니다. 작업 중에는 **작업 취소** 또는 Escape로 멈출 수 있습니다.
5. 실제 절감량을 확인하고 **폴더 열기**로 결과를 찾습니다.

기본 저장 위치는 각 원본 폴더 안의 `Compressed`입니다. 같은 이름이 있으면 번호를 붙입니다. 결과가 원본보다 크거나 같으면 새 파일을 남기지 않고 **원본 유지 · 압축 이득 없음**으로 표시합니다. 완료한 파일은 다시 예상 작업을 눌러도 결과가 사라지지 않습니다.

## 폴더 전체 압축 (v0.2.0)

**폴더 추가**를 누르거나 폴더를 창에 끌어놓으면 하위 폴더까지 영상 파일을 찾습니다. 폴더 안의 영상 수에 500개 제한을 두지 않으며, 파일 정보 확인 후 목록의 영상 전체를 하나씩 순차 압축합니다. 파일과 폴더를 섞어 넣어도 같은 경로는 한 번만 등록합니다.

기본 출력은 선택한 폴더의 `Compressed` 아래에 원래 하위 구조를 유지합니다.

```text
내 영상/2024/clip.mov → 내 영상/Compressed/2024/clip_compressed.mp4
내 영상/2025/clip.mov → 내 영상/Compressed/2025/clip_compressed.mp4
```

저장 위치를 따로 고르면 `저장 위치/원본 폴더 이름/하위 구조`로 저장합니다. 이 경로가 원본 폴더와 같거나 원본을 포함하는 경로라면 폴더 이름에 `_Compressed`를 붙여 분리합니다.

기존 `Compressed` 결과 폴더, 현재 사용자 지정 결과 폴더, 심볼릭 링크는 탐색에서 제외하고 개수를 알려줍니다. `Compressed`라는 폴더 자체를 명시적으로 선택하면 그 폴더 안의 영상은 읽습니다. 영상 외 파일은 변경하거나 복사하지 않습니다. 읽지 못한 하위 폴더는 오류를 안내하며, 분석할 수 없는 영상은 목록에서 원인을 확인할 수 있습니다.

탐색·영상 정보 확인·압축 중 모두 취소할 수 있습니다. 이미 확인한 파일은 목록에 남으며 원본은 유지합니다. 큰 폴더는 목록을 50개씩 보여주지만 압축은 현재 페이지에 관계없이 전체 목록에 적용합니다. 압축 중 처리 개수와 완료 후 실제 절감량 합계를 표시합니다.

## 설치 파일과 지원 상태

| 대상 | 파일 | 현재 상태 |
|---|---|---|
| macOS 26 이상 · Apple Silicon | `src-tauri/target/release/bundle/dmg/Dropper_0.2.0_aarch64.dmg` | 앱 실행 및 실제 UI 압축 흐름 확인 |
| Windows x64 | `artifacts/Dropper_0.2.0_x64-setup.exe` | 교차 컴파일·NSIS 생성, Windows 설치/실행 미검증 |
| macOS 12 이상 · Intel | `src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Dropper_0.2.0_x64.dmg` | Intel 앱·엔진·DMG 생성, 실제 실행 미검증 |

위 경로는 빌드 시 생성되는 위치이며 Git에는 설치 파일·엔진 바이너리·빌드 캐시를 포함하지 않습니다. 로컬 생성물을 정리한 뒤에는 아래 준비·빌드 명령으로 다시 생성할 수 있습니다.

현재 **Apple Silicon** 번들의 Homebrew FFmpeg와 동적 라이브러리는 **macOS 26 이상**을 요구합니다. 앱의 최소 OS 버전도 이에 맞췄습니다. Intel 별도 빌드는 정적 x86_64 엔진을 사용하고 macOS 12 이상을 대상으로 합니다. 각 아키텍처용 설치 파일을 선택해 주세요. 기존 드롭퍼가 실행 중이면 종료한 뒤 새 설치 파일을 여세요.

설치 파일은 개인 로컬 사용을 위한 **인증서 서명·Apple 공증을 하지 않은 빌드**입니다. 외부 게시나 스토어 등록은 수행하지 않았습니다. Windows 설치 프로그램은 사용자 계정의 앱 폴더에 설치하며 WebView2가 없을 때 공식 Microsoft 설치 도우미를 실행합니다. 이 최초 런타임 준비에는 인터넷이 필요할 수 있습니다. 이후 영상 압축은 로컬에서 수행합니다.

## 영상 형식과 보존 정책

- 주요 입력: MP4, MOV, MKV, AVI, WebM, WMV, FLV, MPEG/MPG, TS, MTS, M2TS, M4V, 3GP. 실제 지원 여부는 파일의 내부 코덱과 포함된 FFmpeg 빌드에 따라 달라집니다.
- 기본 출력: MP4 + H.265. 세부 설정에서 H.264·AV1 및 MP4·MKV를 선택할 수 있습니다.
- 해상도와 프레임 타이밍을 기본 유지합니다. 홀수 해상도는 최대 1픽셀 패딩을 추가합니다. 회전 영상은 보이는 방향을 유지해 저장합니다.
- 원본 오디오·자막·첨부를 복사합니다. MP4에 보존할 수 없는 트랙은 자동 모드에서 MKV로 전환합니다. MP4의 `mov_text` 자막을 MKV로 강제 복사하는 조합은 사전에 안내하고 중단합니다.
- HDR10·HLG는 H.265 10비트 경로를 사용합니다. HDR 색 정보와 정적 HDR 메타데이터를 결과에서 확인합니다.
- 감지된 Dolby Vision·HDR10+ 동적 메타데이터, 알파, 인터레이스, 복수 영상/특수 데이터 트랙은 지원 제한을 안내합니다. 자동 SDR 변환이나 조용한 트랙 삭제는 하지 않습니다.
- `화질 우선`도 손실 압축입니다. 영상에 따라 용량 감소가 작거나 압축 이득이 없을 수 있습니다.

예상치는 12초 이하 영상의 전체 또는 긴 영상의 앞·중간·뒤에서 각 4초를 시험 압축해 계산합니다. 표본 영상의 비트레이트와 복사할 오디오 크기에 2%의 컨테이너 여유를 반영하며, 이 값은 검증된 오차 범위가 아닙니다. 오디오 비트레이트가 미상이면 오디오 패킷을 추가 표본 분석합니다. 실제 장면 구성에 따라 오차가 있으며 확정 용량은 전체 압축이 끝난 뒤 알 수 있습니다.

## 개발 실행

필요 도구: Node.js/npm, Rust **1.96 이상**, Python 3.9 이상, 해당 OS의 Tauri 빌드 도구. 개발 실행에는 FFmpeg/ffprobe를 PATH에 준비하거나 엔진을 번들해야 합니다. Windows는 MSVC 빌드 도구와 WebView2가 필요합니다.

```sh
npm ci
# macOS: 로컬 FFmpeg/ffprobe를 앱 리소스로 준비
npm run bundle:media
npm run desktop
```

Windows에서 개발 실행하려면 `npm ci` 후 엔진을 먼저 준비합니다.

```sh
python scripts/fetch-windows-media.py
python scripts/bundle-media.py --windows --source-dir artifacts/windows-media --output src-tauri/resources/media-windows
npm run desktop
```

브라우저 화면만 확인하려면 `npm run dev`를 사용합니다. 브라우저 미리보기는 파일 압축을 실행하지 않으며 데스크톱 앱 사용 안내를 표시합니다.

```sh
# 현재 OS 설치 파일 생성
npm run bundle

# 테스트 및 정적 검사
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
python3 -m unittest discover -s tests/packaging -v

# 브라우저 UI 검사 (최초 한 번 Chromium 설치)
npx playwright install chromium
npm run test:ui
```

`DROPPER_BROWSER`에 기존 Chromium 실행 파일 경로를 주면 UI 검사에서 그 브라우저를 사용할 수 있습니다. 제품 실행 파일은 이 설정을 사용하지 않습니다.

## 엔진과 플랫폼별 빌드

`scripts/bundle-media.py`는 macOS의 로컬 FFmpeg/ffprobe 및 비시스템 라이브러리의 복사본을 준비하고 상대 경로로 연결합니다. 원래 설치된 엔진은 변경하지 않습니다. 릴리스 앱은 시스템 FFmpeg로 자동 대체하지 않습니다.

Windows 엔진은 `scripts/windows-media.json`의 SHA-256으로 고정한 공개 FFmpeg 배포물을 사용합니다. 원격 latest 파일이 바뀌어 체크섬이 다르면 다운로드를 수락하지 않습니다. 버전/체크섬을 검토해 함께 갱신해야 합니다.

macOS에서 Windows 빌드를 재현하려면 Rust `x86_64-pc-windows-msvc` 타깃, cargo-xwin, LLVM의 clang-cl/llvm-rc, lld-link, NSIS가 필요합니다. 해당 도구를 PATH에 넣은 뒤 다음 명령을 사용합니다.

```sh
sh scripts/build-windows-cross.sh
```

SDK는 xwin이 정상 처리한 `10.0.22621`로 고정합니다. `artifacts/windows-media`와 생성된 엔진 폴더는 Git에서 제외합니다. Mac과 Windows 엔진은 각각 별도 폴더와 플랫폼 설정으로 묶어 아키텍처가 섞이지 않도록 했습니다.

Intel macOS 교차 빌드는 `rustup target add x86_64-apple-darwin`으로 타깃을 준비한 뒤 `sh scripts/build-mac-intel.sh`를 실행합니다. FFmpeg 공식 다운로드 페이지가 안내하는 Evermeet의 고정 버전 x86_64 정적 엔진을 `scripts/mac-intel-media.json`의 체크섬으로 검증합니다. Apple Silicon 리소스를 포함하지 않도록 Intel 설정에서 리소스 매핑을 교체하며, 다른 아키텍처의 엔진을 실행 검증한 것으로 처리하지 않습니다. 포함된 인코더에 따라 Intel 빌드에서 AV1이 제공되지 않을 수 있습니다.

## 구조

| 경로 | 역할 |
|---|---|
| `src-tauri/src/media.rs`, `command.rs`, `policy.rs` | 미디어 검사, 압축 정책과 명령 인자, 용량 계산 |
| `src-tauri/src/process.rs`, `engine.rs` | 취소·타임아웃과 실제 FFmpeg 변환, 임시 파일·출력 검증 |
| `src-tauri/src/jobs.rs`, `desktop.rs` | 등록 파일·작업 대기열, Tauri 명령과 상태 이벤트 |
| `src/` | React UI, 예상/실제 결과 상태와 간결한 디자인 |
| `scripts/` | 엔진 준비와 플랫폼별 패키징 |

원본이 추가 후 변경되면 작업을 중단하고 재추가를 안내합니다. 재추가 시 분석 결과와 이전 예상치를 갱신합니다. 셸 명령 문자열을 만들지 않고 고정 인자를 전달하며 등록되지 않은 파일 ID로 작업을 실행할 수 없습니다.

외부 구성 요소와 배포 전 확인 사항은 [THIRD_PARTY.md](THIRD_PARTY.md)에 정리합니다.
